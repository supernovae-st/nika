# Nika LSP v2.0 — Definitive Master Plan

> **Version:** 2.1 | **Date:** 2026-03-19 (updated: same day, pass 6)
> **Supersedes:** All previous LSP plans (see [Archives](#18-archives))
> **Research:** 10 audit agents, 6 brainstorm docs, 600K+ chars of analysis
> **Baseline:** v0.34.0 (6,200+ tests via `--lib`, 21 media tools, vision pipeline, DAG v3)
> **Status:** **APPROVED** — single source of truth for all LSP work

---

## Table of Contents

1. [Baseline v0.34.0](#1-baseline-v0340)
2. [Confirmed Decisions](#2-confirmed-decisions)
3. [Architecture: 3-Crate Design](#3-architecture-3-crate-design)
4. [Track A: Foundation](#4-track-a-foundation) (7 PRs, ~14-16 weeks)
5. [Track B: Quick Wins](#5-track-b-quick-wins) (4 PRs, parallel)
6. [Track C: Intelligence](#6-track-c-intelligence) (10 PRs, post-foundation)
7. [Track D: Magic](#7-track-d-magic) (6 PRs, post-intelligence)
8. [Track E: Ecosystem](#8-track-e-ecosystem) (7 PRs, ongoing)
9. [Novel Features Catalog](#9-novel-features-catalog) (30 features)
10. [Error Code Coverage Map](#10-error-code-coverage-map) (101 codes)
11. [Testing Strategy](#11-testing-strategy)
12. [VS Code Extension Roadmap](#12-vs-code-extension-roadmap)
13. [Performance Targets](#13-performance-targets)
14. [UX Philosophy](#14-ux-philosophy)
15. [Security Model](#15-security-model)
16. [Accessibility](#16-accessibility)
17. [Code Review Methodology](#17-code-review-methodology)
18. [Archives](#18-archives)
19. [Grand Summary](#grand-summary)

---

## 1. Baseline v0.34.0

What we START with — verified by 10 parallel audit agents on 2026-03-19.

### Codebase Metrics

| Component | Status | LOC | Tests |
|-----------|--------|-----|-------|
| Embedded LSP (`src/lsp/`) | 6 handlers, 15 files | 10,160 | 210 |
| Standalone LSP (`nika-lsp/`) | 4 handlers, 11 files | 4,019 | 73 |
| AST pipeline (Raw→Analyzed→Lower) | 3-phase, stable | ~15,000 | 733 |
| model_intel.rs (model catalog) | **Committed**, 33 cloud + 15 native models | 1,413 | 65 |
| display.rs (DAG v3 rendering) | **Committed**, live ANSI, verb icons | 754 | — |
| Binding system (templates, transforms) | 31 transforms, RFC 9535 JSONPath | ~4,500 | ~200 |
| Media pipeline (21 tools, 3 tiers) | **Complete**, CAS, zstd, vision, QR, IQA | ~6,000 | ~2,000 |
| Provider system (7 LLM, 11 MCP, 1 native) | cost.rs, 48 MCP aliases | ~2,000 | ~100 |
| DAG module (flow, indexed, stable, validate) | Kahn topo sort, cycle detection | ~3,500 | ~100 |
| Event system (37 event kinds) | Thread-safe, broadcast | ~1,500 | ~50 |
| Full test suite | **6,093 passing** (`cargo test --lib`) | — | 6,093 |
| Quality sweep (16 tasks) | **ALL 16 DONE** | — | — |

### Key Leverageable Assets

1. **model_intel.rs** (1,413 LOC, 65 tests) — 33 models, 10 capability flags (incl. VISION), pricing via cost.rs, deprecation tracking, alternatives, compatibility checker
2. **display.rs** (754 LOC) — Verb icons (⚡📟🛰️🔌🐔), box-drawing, DAG rendering v3
3. **31 transforms** — upper, lower, sort, first, keys, flatten, round, join, split, shell, default, etc.
4. **48 MCP aliases** — npm package mappings for 6 categories
5. **ContentPart pipeline** — Text/Image/ImageUrl through Raw→Analyzed→Lower
6. **AnalyzeError** — 8 kinds (NIKA-140..151) with spans, suggestions, notes
7. **101 error codes** — NIKA-000 through NIKA-309, all implemented
8. **tree-sitter-yaml 0.7** — Already in deps (TUI syntax highlighting)
9. **proptest + insta** — 34 proptests, 28 insta snapshots
10. **criterion** — Infrastructure present (354 LOC), dormant (0 active benchmarks)

### LSP Features Implemented

| Feature | Embedded | Standalone | Total Tests |
|---------|:--------:|:----------:|:-----------:|
| Completion | ✅ | ✅ | 24 |
| Hover | ✅ | ✅ | 20 |
| Go-to-Definition | ✅ | ✅ | 27 |
| Code Action | ✅ | ❌ | 20 |
| Semantic Tokens | ✅ | ❌ | 35 |
| Document Symbols | ✅ | ❌ | 10 |
| Diagnostics | ✅ | ✅ | ~10 |

**Implemented: 7 features.** Not implemented (17 remaining):
Rename, References, Inlay Hints, Code Lens, Folding, Formatting, Document Link, Signature Help, Call Hierarchy, Highlight, Selection Range, Workspace Symbol, Linked Editing Range, On-Type Formatting, Document Color, Color Provider, Moniker.

**Total LSP feature space: 24** (7 implemented + 17 not implemented).

### Code Duplication

**~65% overlap** between embedded and standalone LSP:
- Position conversion (427 vs 881 LOC, same logic)
- Completion providers (726 vs 1,184 LOC, same verbs/schemas)
- Hover docs (481 vs 1,007 LOC, same content)
- Diagnostics (216 LOC vs inline in server.rs)

### Standalone-Unique Features (to merge into core)

| Module | LOC | Tests | Unique Value |
|--------|-----|-------|-------------|
| node_context.rs | 587 | 14 | 9-variant AstContext enum, AST+fallback detection |
| template_validation.rs | 355 | 7 | `{{with.*}}` regex validation with positions |
| mcp_discovery.rs | 268 | 8 | 12 NovaNet tools (hardcoded) |
| document.rs | 160 | 5 | Rope-based incremental edits (ropey) |
| position.rs | 427 | 13 | ByteOffset type, word boundary detection |
| ast_integration.rs | 360 | 9 | ParsedWorkflow, TaskDefinition, fallback parsing |

---

## 2. Confirmed Decisions

| Decision | Answer | Rationale |
|----------|--------|-----------|
| Crate design | **3 crates** (nika-lsp-core + 2 thin binaries) | Eliminates 65% duplication |
| Incremental cache | **Generation-based WorldDatabase** (not salsa) | 90% benefit at 10% complexity |
| Error recovery | **tree-sitter-yaml bridge** (structure-only) | Only YAML parser with recovery |
| LSP framework | **tower-lsp-server 0.23** (in handler migration PR) | Community fork, Rust 2024 edition |
| model_intel.rs | **Already committed** — move to nika-lsp-core | PR:foundation |
| PR naming | **Named PRs** (not numbered) | No collision with media PR1-PR4 |
| Plan structure | **Single source of truth** (this document) | Replaces 4 old plans |
| Timeline | **22-26 weeks for Tracks A+B+C** (realistic with setbacks) | Validated by rust-architect risk analysis |
| Standalone binary | **Keep both** entry points | nika lsp (integrated) + nika-lsp (lightweight) |
| REPL mode | **Full Jupyter notebook** (Track D) | VS Code Notebook API native |
| DAG visual | **Bidirectional** (drag → YAML, YAML → graph) | Track D |
| Formatting | **Opinionated** (one true format) | Track E, like Prisma |
| Quality sweep v1 | **Archived** (16/16 done) | New gaps in this plan |

---

## 3. Architecture: 3-Crate Design

```
nika-lsp-core/              # Protocol-agnostic intelligence (NO lsp-types dep!)
  ├── Own types: Diagnostic, TextRange, Severity, AnalysisSnapshot
  ├── WorldDatabase (generation-based, NOT salsa)
  ├── Handlers: pure fn(db, file, pos) -> Response
  ├── tree-sitter-yaml for error recovery (STRUCTURE-ONLY)
  ├── model_intel.rs (moved here)
  └── Consumed by: LSP, TUI, CLI (all 3)

nika (main binary)
  ├── src/lsp/ → thin tower-lsp-server 0.23 shim → delegates to nika-lsp-core
  ├── src/tui/ → embeds nika-lsp-core directly (zero IPC)
  └── nika check → WorldDatabase one-shot

nika-lsp (standalone binary)
  └── ~30 lines: tower-lsp wiring + stdio
```

### Key Invariants

1. **nika-lsp-core does NOT depend on lsp-types** — own Diagnostic/TextRange/Severity
2. **tree-sitter bridge is STRUCTURE-ONLY** — never trust ts-yaml value types (YAML 1.1 vs 1.2)
3. **Inter-file deps**: `include_dependencies: DashMap<Url, Vec<Url>>` + `workspace_revision: AtomicU64`
4. **Handlers are pure functions** — `fn(db, file, pos) -> Response`, no state, no async, testable
5. **Error codes**: always `NikaError` with `NIKA-XXX`, never `anyhow`

### WorldDatabase — The Heart

```rust
pub struct WorldDatabase {
    revision: AtomicU64,
    texts: DashMap<Url, (Arc<str>, u64)>,
    parsed: DashMap<Url, Versioned<ParsedDocument>>,
    analyzed: DashMap<Url, Versioned<AnalyzedSnapshot>>,
    line_indices: DashMap<Url, Versioned<LineIndex>>,
    position_index: DashMap<Url, Versioned<PositionIndex>>,
    uri_to_key: DashMap<Url, FileKey>,
    key_to_uri: DashMap<FileKey, Url>,
    workspace_roots: RwLock<Vec<PathBuf>>,
    include_dependencies: DashMap<Url, Vec<Url>>,
    workspace_revision: AtomicU64,
    mcp_tool_cache: DashMap<String, Vec<McpToolSchema>>,
}
```

### Error Recovery — 3-Layer Strategy

```
Document text (possibly broken YAML)
  │
  ├─→ [tree-sitter-yaml] → CST (ALWAYS available, incremental)
  │     └─→ [bridge.rs] → PartialWorkflow (task IDs, verbs, spans)
  │                        Used for completions/hover on broken code
  │
  ├─→ [marked_yaml] → RawWorkflow (when YAML is valid)
  │     └─→ [analyzer] → AnalyzedWorkflow + errors (full semantics)
  │
  └─→ Context detection: AST > CST > line fallback (3 strategies)
```

### Debouncing & Cancellation

```rust
pub struct AnalysisScheduler {
    edit_notify: Arc<Notify>,
    cancel: Arc<RwLock<CancellationToken>>,
    debounce: Duration,  // 150ms for diagnostics
}
```

- **Read path** (completion, hover, definition): Use latest cached snapshot. Never wait. < 50ms.
- **Write path** (did_change): Queue analysis via scheduler. 150ms debounce. Cancel on new edit.

---

## 4. Track A: Foundation

> **Goal:** Zero duplication, error recovery, cached performance, standard LSP features.
> **Timeline:** ~14-16 weeks, 7 PRs, sequential.
> **Exit criteria:** 0% duplication, <50ms completions, error recovery works, 21/24 standard features.

### PR:extract-ast — Lightweight nika-core Crate (PREREQUISITE)

**Week 0-1 | 0 new tests (refactor only) | Critical path**

**Why this PR exists:** The plan's 3-crate design requires `nika-lsp-core` to depend on `nika`. But `nika`'s Cargo.toml has 260+ lines of dependencies — it pulls in the entire runtime (tokio, reqwest, rmcp, rig-core, image, c2pa, zstd, etc.). A lightweight `nika-lsp-core` is impossible if it inherits this graph.

**Solution:** Extract the AST/analysis layer into a `nika-core` crate:

```
nika-core/                       # NEW: Lightweight analysis-only crate
  src/
    ast/                         # MOVED from nika/src/ast/
      raw/, analyzed/, analyzer/
      lower.rs, content.rs, schema.rs
    source/                      # MOVED from nika/src/source/
      span.rs, registry.rs
    error.rs                     # MOVED from nika/src/error.rs
    core/                        # MOVED from nika/src/core/
      providers.rs, models.rs, mcp_aliases.rs
    binding/types.rs             # MOVED: BindingPath, BindingSource, Transform types only
    dag/                         # MOVED from nika/src/dag/ (validation only)
```

**Dependencies (minimal):**
```toml
[dependencies]
marked-yaml = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
indexmap = "2"
rustc-hash = "2"
thiserror = "2"
miette = { version = "7", features = ["fancy"] }
tracing = "0.1"
```

**No tokio, no reqwest, no rig-core, no image, no MCP.** Compile time: seconds, not minutes.

**Migration:** `nika` crate becomes `nika-core` + runtime layer. All `use nika::ast::` becomes `use nika_core::ast::`. This is a mechanical refactor with zero behavior change.

**Checkpoint:** `cargo test -p nika-core --lib` passes all existing AST tests (733).

---

### PR:tower-lsp-upgrade — Migrate to tower-lsp-server 0.23

**Week 1-2 | 0 new tests (migration only)**

**Why separate from handler migration:** Bundling a framework migration with a 3,000 LOC handler merge is a review nightmare.

**Breaking changes (verified from crates.io + GitHub):**

| Change | Impact | Files |
|--------|--------|-------|
| **Crate rename** | `tower-lsp` → `tower-lsp-server` in Cargo.toml, `tower_lsp::` → `tower_lsp_server::` | All LSP files |
| **lsp-types → ls-types** | `lsp_types::` → `ls_types::` (tower-lsp-community fork, v0.0.6) | All handlers |
| **Url → Uri** | `lsp_types::Url` (url crate) → `ls_types::Uri` (fluent-uri). Use `UriExt::to_file_path()` | server.rs, definition.rs |
| **async_trait removed** | Remove `#[async_trait]` from `impl LanguageServer`. Native RPITIT. | server.rs, backend.rs |
| **Rust 1.85+ MSRV** | Edition 2024, Rust 1.85+ required | Cargo.toml rust-version |
| **Tower 0.5** | tower `^0.4` → `^0.5` | Cargo.toml |
| **Notebook support** | New `notebook_did_open/change/save/close` methods (default impls) | Opt-in |

**Note:** ls-types is at 0.0.x (unstable API). This is the trade-off of Option C. The benefit: we're on the actively maintained fork with LSP 3.18 support.

**Migration checklist:**
```
[ ] Update Cargo.toml: tower-lsp = "0.20" → tower-lsp-server = "0.23"
[ ] Remove async-trait from dependencies
[ ] Update all imports: tower_lsp:: → tower_lsp_server::
[ ] Update all imports: lsp_types:: → ls_types::
[ ] Remove #[async_trait] from LanguageServer impl
[ ] Replace Url with Uri throughout LSP code (use UriExt)
[ ] Verify Rust toolchain >= 1.85
[ ] Run cargo test --lib --features lsp
```

**Scope:** Only `tools/nika/src/lsp/server.rs` and `tools/nika-lsp/src/backend.rs`. No handler logic changes.

**Checkpoint:** Both `nika lsp` and `nika-lsp` start and respond to initialize.

---

### PR:foundation — nika-lsp-core + WorldDatabase

**Weeks 3-5 | 80+ tests | ~1,500 LOC new**

Create the `nika-lsp-core` crate with core infrastructure. No handlers yet. Both existing LSPs untouched.

**Files created (12):**
```
nika-lsp-core/
  Cargo.toml
  src/
    lib.rs
    db.rs              # WorldDatabase + Versioned<T> + generation tracking
    document.rs        # Rope-based document (from nika-lsp/document.rs)
    position.rs        # LineIndex for O(log n) offset/position conversion
    protocol.rs        # Span → Range, own Diagnostic/TextRange/Severity
    workspace.rs       # Multi-file workspace + include resolution
    index/
      mod.rs
      ast_index.rs     # Per-file AST cache with FileKey
      symbol_index.rs  # Cross-file symbol table (tasks, MCP servers)
```

**Key implementations:**
- `LineIndex::new(text)` — O(n) once, O(log n) per lookup. Handles `\r\n` and UTF-16 surrogates.
- `PositionIndex::build(workflow)` — Sorted span entries for O(log n) node-at-offset lookup
- Migrate conversion.rs (881 LOC, 31 tests, correct surrogate handling)
- Migrate position.rs (427 LOC, 13 tests) from standalone
- Add proptest for position roundtrip invariance

**Dependencies:**
```toml
[dependencies]
nika-core = { path = "../nika-core" }  # AST + analysis only (NOT full nika!)
ropey = "1.6"
dashmap = "6"
parking_lot = "0.12"
rustc-hash = "2"
tracing = "0.1"
# tree-sitter added in PR:error-recovery:
# tree-sitter = "0.24"
# tree-sitter-yaml = "0.7"

[dev-dependencies]
pretty_assertions = "1"
insta = { version = "1.34", features = ["yaml"] }
proptest = "1"
criterion = { version = "0.5", features = ["async_tokio"] }
```

**Note:** `nika-lsp-core` depends on `nika-core` (lightweight, <10 deps), NOT on `nika` (260+ deps). This is the architectural boundary that makes the 3-crate design meaningful.

**Migration path from current state:**
1. `AstIndex` (DashMap<Url, CachedAst>) → absorbed into `WorldDatabase`
2. `DocumentStore` (HashMap<Url, String>) → replaced by `WorldDatabase.texts` (DashMap<Url, Arc<str>>)
3. Both coexist during PR:handler-migration-wire (dual-write)
4. Old stores deleted in PR:handler-migration-delete

**Note:** conversion.rs has 31 verified test functions (the old plan incorrectly claimed 142).

**Checkpoint:** `cargo test -p nika-lsp-core --lib`

---

### PR:error-recovery — tree-sitter-yaml Bridge

**Weeks 4-5 | 50+ snapshot tests | ~800 LOC new**

LSP features work on broken YAML.

**Files created (3):**
```
nika-lsp-core/src/parse/
  mod.rs
  recovery.rs    # tree-sitter-yaml incremental parser
  bridge.rs      # CST → PartialWorkflow (STRUCTURE-ONLY)
```

**Key types:**
```rust
pub struct PartialWorkflow {
    pub schema: Option<String>,
    pub workflow_name: Option<String>,
    pub task_ids: Vec<PartialTask>,
    pub mcp_servers: Vec<String>,
    pub context_files: Vec<String>,
    pub has_errors: bool,
}

pub struct PartialTask {
    pub id: String,
    pub verb: Option<String>,   // "infer", "exec", etc.
    pub has_content: bool,      // vision content: block present
    pub span: TextRange,
}
```

**MANDATORY invariant:** Bridge ONLY extracts structural info (key positions, indentation, block structure). It NEVER trusts tree-sitter's value interpretation. All value semantics come from marked_yaml.

**Broken YAML fixture corpus (50+ files):**
```
fixtures/broken/
  missing-colon.nika.yaml         # tasks\n  research\n    infer
  incomplete-task.nika.yaml       # tasks:\n  - id: \n    infer:
  duplicate-verb.nika.yaml        # infer: ... and exec: ... in same task
  mixed-indentation.nika.yaml     # tabs + spaces
  truncated-file.nika.yaml        # file ends mid-value
  unicode-keys.nika.yaml          # emoji in task names
  empty-document.nika.yaml        # empty file
  comments-only.nika.yaml         # only # comments
  multi-document.nika.yaml        # --- separators
  missing-schema.nika.yaml        # no schema: field
  nested-broken.nika.yaml         # broken inside with: block
  broken-template.nika.yaml       # {{with. (unclosed)
  broken-content.nika.yaml        # content: with incomplete image part
  broken-mcp.nika.yaml            # mcp: with missing server fields
  broken-retry.nika.yaml          # retry: with invalid fields
  ... (35 more, categorized by error type)
```

All fixtures get insta snapshots verifying PartialWorkflow extraction.

**Checkpoint:** All fixtures produce PartialWorkflow (never panic).

---

### PR:handler-migration-wire — Delegation Layer

**Weeks 6-8 | 250+ tests | ~3,000 LOC**

All handler logic in core crate. Both entry points delegate. Old code still exists alongside.

**Files created in nika-lsp-core (16):**
```
nika-lsp-core/src/
  analysis/
    mod.rs
    context.rs       # 3-strategy cursor context detection (from node_context.rs)
    diagnostics.rs   # NIKA error → own Diagnostic (ALL 101 codes)
    template.rs      # {{with.*}} validation (from template_validation.rs)
  handlers/
    mod.rs
    completion.rs    # Merged from both implementations
    hover.rs         # Merged + model_intel hover_markdown()
    definition.rs    # Go-to-def: tasks, bindings, includes, imports
    code_action.rs   # Quick fixes: 20+ (expanded from 4)
    semantic_tokens.rs
    symbols.rs
    diagnostics.rs   # Publishing logic
  knowledge/
    mod.rs
    schema.rs        # Nika schema knowledge (verbs, fields, transforms)
    mcp_tools.rs     # MCP tool catalog (from mcp_discovery.rs)
    model_intel.rs   # MOVED from src/lsp/model_intel.rs
    providers.rs     # Provider catalog (wraps core::providers)
    transforms.rs    # 27 transform definitions for completions
```

**Merge strategy per handler:**

| Handler | Base | Merge From Standalone | Additions (genuinely new) |
|---------|------|----------------------|--------------------------|
| completion | Embedded (1,184 LOC) | **13-item structured output completions** (7 JSON Schema types + 6 snippets), **McpTool context** (tool completions after server selected), **Schema version completions** (9 versions with descriptions), AST-based context detection (node_context.rs) | depends_on task ID suggestions, 18 nika:* tools, 31 transforms after `\|`, content: field completions |
| hover | Embedded (1,007 LOC) | **RETRY_DOCS** (retry documentation, absent from embedded), FLOWS_DOCS (deprecated), USE_DOCS (old binding syntax) | model_intel `hover_markdown()` (trigger: cursor on model name value), transform docs, ContentPart docs |
| definition | Embedded (946 LOC) | — (standalone has **no definition.rs**; handled in backend.rs via AST) | Cross-file task definition (jump to task in included file, not just file start — **partial support already exists** for include path → file navigation) |
| code_action | Embedded (1,135 LOC) | — | Expand from **7** (4 quick fixes: NIKA-140/141/142/145 + 3 refactoring: expand shorthand infer/exec, add with: block) to 20+ fixes. **Note:** existing 4 NIKA codes MUST be preserved during migration. |
| semantic_tokens | Embedded (1,181 LOC) | — | **ContentPart tokens** (content:, type:, detail:, source:). Note: template `{{...}}` already tokenized as STRING. **Bug fix needed:** MCP server name tokenization fails in @0.12 format (expects old `servers:` sub-key). |
| symbols | Embedded (823 LOC) | — | **Note:** MCP servers, context files, and imports are **already in the outline** (both text-based and AST-aware paths). Actual additions: `inputs:` block, `edges:` section, for_each as task child. **Decision needed:** canonicalize on `DocumentSymbol` (hierarchical) vs `SymbolInformation` (flat). |
| context | — | node_context.rs (587 LOC, **14 tests**) | Replace embedded's line/indent heuristics with AST-based detection. Add CST fallback strategy, ContentPart context variant. |
| template | — | template_validation.rs (355 LOC, **7 tests**) | 31 transform validation, type checking |
| mcp_tools | — | mcp_discovery.rs (268 LOC, **8 tests**) | 18 nika:* tools (replace hardcoded 12 NovaNet), 48 MCP aliases, dynamic discovery stub |

**Translation layer note:** Handlers currently return `lsp-types`/`ls-types` types directly (~25 types: CompletionItem, Hover, Location, etc.). The `protocol.rs` module in nika-lsp-core defines protocol-agnostic equivalents:

```rust
// nika-lsp-core/src/protocol.rs — own types
pub struct CompletionEntry { label, insert_text, is_snippet, detail, documentation, kind, sort_priority }
pub struct HoverResult { contents: String, range: Option<TextRange> }
pub struct DefinitionResult { file_key: FileKey, range: TextRange }
pub struct TextPosition { line: u32, character: u32 }
pub struct TextRange { start: u32, end: u32 }
pub enum Severity { Error, Warning, Info, Hint }
pub enum CompletionKind { Keyword, Property, Verb, TaskRef, Variable, Module, Transform, Value, Snippet }
```

**Decision (updated after rust-architect review): Option B — depend on ls-types directly in nika-lsp-core.** ls-types is lightweight (~50KB, serde+url+bitflags only). Handlers return ls-types structs directly. Zero translation layer. The TUI/CLI can still use WorldDatabase + analysis without touching handler response types. This saves ~500 LOC and 2 days of work vs the original Option A (own types + From impls).

The WorldDatabase, PositionIndex, LineIndex, parse/analysis pipeline remain protocol-agnostic. Only handler functions that assemble CompletionItem lists use ls-types.

**See:** `docs/plans/2026-03-19-nika-lsp-v2-implementation-reference.md` for complete Rust code snippets (FileSnapshot, PositionIndex, CursorContext, AnalysisScheduler, translation layer).

**tower-lsp upgrade:** Now in its own **PR:tower-lsp-upgrade** (not bundled here).

**tower-lsp upgrade happens HERE:** tower-lsp 0.20 → tower-lsp-server 0.23.

**Checkpoint:** Feature parity verified — all existing tests pass through delegation.

---

### PR:handler-migration-delete — Delete Old Code

**Week 9 | Same 250+ tests pass | Net LOC decrease**

Only after PR:handler-migration-wire proven for 1+ week in use.

**Files deleted (19):**
```
# Embedded LSP (10 files)
nika/src/lsp/handlers/completion.rs
nika/src/lsp/handlers/hover.rs
nika/src/lsp/handlers/definition.rs
nika/src/lsp/handlers/code_action.rs
nika/src/lsp/handlers/semantic_tokens.rs
nika/src/lsp/handlers/symbols.rs
nika/src/lsp/ast_index.rs
nika/src/lsp/document_store.rs
nika/src/lsp/conversion.rs
nika/src/lsp/utils.rs

# Standalone LSP (9 files)
nika-lsp/src/completion.rs
nika-lsp/src/hover.rs
nika-lsp/src/diagnostics.rs
nika-lsp/src/node_context.rs
nika-lsp/src/mcp_discovery.rs
nika-lsp/src/template_validation.rs
nika-lsp/src/position.rs
nika-lsp/src/document.rs
nika-lsp/src/ast_integration.rs
```

**Net effect:** ~5,000 LOC deleted, ~3,000 LOC in nika-lsp-core → **-2,000 LOC net, zero duplication**.

**Checkpoint:** `git diff --stat` shows net LOC decrease. All tests pass.

---

### PR:standard-features — Inlay Hints, Code Lens, Rename, References, + 5 more

**Weeks 10-12 | 70+ new tests**

Complete the standard LSP feature set + 5 features missing from v1 plans.

**14 new handlers:**

**inlay_hints.rs (~500 LOC):**

| Hint | Default | Rationale |
|------|---------|-----------|
| Dependency chain | **ON** | Invisible without scrolling |
| Timeout clarification | **ON** | `30` is ambiguous (seconds? ms?) |
| Binding source | **ON** | Helps beginners |
| Verb badge | OFF | Already visible as YAML key |
| Template preview | OFF | Dynamic, can mislead |
| Provider | OFF | Debug-only |
| Cost estimate | OFF | Needs accuracy validation |
| Task count (at `tasks:`) | OFF | Noise for small workflows, opt-in for 50+ |
| for_each info | OFF | Count often unknowable at edit time |

**code_lens.rs (~400 LOC):**

| Position | Lens | Command |
|----------|------|---------|
| `schema:` line | Run Workflow / Validate / Show DAG | nika.run / nika.check / nika.dag |
| Each task `id:` | Run from here / N dependents | nika.runTask / nika.showDependents |
| `mcp:` section | Test connections | nika.testMcp |

Security: Run gated by VS Code **Workspace Trust**. Untrusted = Validate only.

**rename.rs (~300 LOC):**
- Rename task ID → update ALL references: depends_on, with bindings, template refs
- Cross-file rename for include: references (uses workspace symbol index)
- Prepare rename validation (reject invalid names)

**references.rs (~200 LOC):**
- Find all references to a task ID (depends_on, with, templates)
- Sources: explicit deps, implicit deps, template expressions

**folding.rs (~150 LOC):**
- Collapse each task block, verb block, multiline strings (`|`, `>`), with: block, mcp: section

**document_link.rs (~150 LOC):**
- `include: ./lib.nika.yaml` → Ctrl+Click opens file
- `fetch: { url: https://... }` → Ctrl+Click opens URL
- `pkg://supernovae/seo-tools` → Ctrl+Click opens registry

**highlight.rs (~100 LOC):**
- Cursor on task ID → highlight all references in document

**selection_range.rs (~100 LOC):**
- Smart expand/shrink: cursor → field → verb → task → tasks: → workflow
- (rust-analyzer's killer UX feature)

**workspace_symbol.rs (~150 LOC):**
- Search tasks across ALL .nika.yaml files
- Returns: task ID + file path + verb type

**linked_editing.rs (~100 LOC):**
- Cursor on task `id:` field → mirror cursors on all depends_on/with refs
- More ergonomic than full rename for quick task ID changes

**on_type_formatting.rs (~100 LOC):**
- After `:` + Enter → auto-indent by 2 spaces
- After `{{` → auto-close `}}`
- After `|` or `>` in multiline → maintain indentation
- Prevents NIKA-160 (syntax) errors from indentation mistakes

**document_color.rs (~80 LOC):**
- Verb colors inline: 🟣 infer, 🟢 exec, 🔵 fetch, 🟡 invoke, 🔴 agent
- Provider colors: 🟣 Anthropic, 🟢 OpenAI, 🔵 Mistral, etc.
- Decorations via `textDocument/documentColor`

**signature_help.rs (~200 LOC):**
- For `invoke:` blocks → show tool parameter signatures
- Trigger on `params:` → required/optional params with types
- For `fetch:` blocks → show HTTP method + headers

**call_hierarchy.rs (~200 LOC):**
- Incoming: "What tasks depend on this one?" (dependents)
- Outgoing: "What does this task depend on?" (dependencies)
- Maps perfectly to DAG edges — unique among LSPs

**Capabilities update:** Add all 14 new providers to capabilities.rs.

**Track A exit criteria:**
- 0% code duplication
- Error recovery on broken YAML (50+ fixtures)
- <50ms completions (cached)
- 21/24 standard LSP features (baseline 7 + 14 new)
- 400+ LSP tests total
- 8,400+ total tests

---

## 5. Track B: Quick Wins

> **Goal:** Immediate UX improvements, no architecture dependency.
> **Timeline:** Start immediately, parallel to Track A.
> **Can be done by a separate contributor/agent.**

### PR:vscode-polish — Extension from 2/10 to 6/10

**~2 days work**

**Snippets (7)** — `editors/vscode/snippets/nika.json`:
- `workflow` → complete workflow scaffold
- `infer` → infer task with model/prompt placeholders
- `exec` → exec task with command/shell
- `fetch` → fetch task with url/method/headers
- `invoke` → invoke task with server/tool/params
- `agent` → agent task with prompt/model/max_turns
- `with` → with binding block

**Commands (4)** — register in package.json + extension.ts:
- `nika.run` → spawn `nika run <file>` in terminal
- `nika.check` → spawn `nika check <file>`, show diagnostics
- `nika.restartServer` → restart LSP server
- `nika.newWorkflow` → create new .nika.yaml from template

**Keybindings (2):**
- `Ctrl+Shift+R` → nika.run (when editorLangId == nika)
- `Ctrl+Shift+V` → nika.check (when editorLangId == nika)

**Status bar (1):**
- Show "Nika LSP ● Running" or "● Error" in bottom right
- Click → show output channel

**Icons (2):**
- `icons/nika-light.svg` — white butterfly for light themes
- `icons/nika-dark.svg` — colored butterfly for dark themes

**Grammar fixes:**
- Add multiline string support (`|`, `>`)
- Add `edges:`, `skills:` as recognized fields
- Add `content:` as verb sub-field (for vision)

**Settings (7 new):**
- `nika.inlayHints.dependencies.enabled` (default: true)
- `nika.inlayHints.timeout.enabled` (default: true)
- `nika.inlayHints.binding.enabled` (default: true)
- `nika.codeLens.run.enabled` (default: true)
- `nika.codeLens.validate.enabled` (default: true)
- `nika.diagnostics.level` (essential/recommended/comprehensive)
- `nika.formatting.enabled` (default: true)

---

### PR:error-coverage — 13% → 40%

**~3 days work**

Surface the most impactful error codes as LSP diagnostics with quick fixes.

**Phase 1 targets (from 13 to ~40 codes):**

| Code | Error | Quick Fix |
|------|-------|-----------|
| NIKA-001 | Workflow parse error | Show parse error location |
| NIKA-002 | Invalid schema version | Insert latest schema |
| NIKA-010-014 | Schema validation | Suggest correct type/field |
| NIKA-020 | DAG cycle | Remove one edge (suggest which) |
| NIKA-021 | Missing dependency | Add depends_on |
| NIKA-030-033 | Provider errors | Suggest valid providers, env var instructions |
| NIKA-041-043 | Template/binding errors | Fix syntax, suggest aliases |
| NIKA-050 | Unknown task ref | "Did you mean?" fuzzy match |
| NIKA-051 | Duplicate task ID | Rename suggestion |
| NIKA-055 | Invalid task ID format | Show allowed characters |
| NIKA-070-074 | With block errors | Auto-generate with: block |
| NIKA-080-082 | DAG with: validation | Fix upstream references |
| NIKA-160-164 | Parse syntax errors | Insert missing `:`, fix indent |

**Elm-style error messages:**
```
error[NIKA-050]: Unknown task reference

  --> workflow.nika.yaml:12:18
   |
12 |     depends_on: [reserach]
   |                  ^^^^^^^^ task 'reserach' not found
   |
   = did you mean 'research'?
   = available tasks: research, write_article, publish
```

---

### PR:benchmarks — Criterion + Fixtures

**~1 day work**

Activate the dormant criterion infrastructure + add broken YAML fixtures.

**Benchmarks to activate in `tests/benchmarks/micro_benchmarks.rs`:**
```rust
fn bench_completion_100_tasks(c: &mut Criterion) { ... }  // Target: < 5ms
fn bench_hover_100_tasks(c: &mut Criterion) { ... }       // Target: < 3ms
fn bench_parse_100_tasks(c: &mut Criterion) { ... }       // Target: < 10ms
fn bench_analyze_100_tasks(c: &mut Criterion) { ... }     // Target: < 20ms
fn bench_position_conversion(c: &mut Criterion) { ... }   // Target: < 1μs
```

**Run:** `cargo bench --bench micro_benchmarks`

**Fixtures:** Create 50+ broken YAML files in `tests/fixtures/broken/` (see PR:error-recovery).

---

### PR:context-inputs — Context File + Input Intelligence

**~2 days work**

Quick wins that don't need nika-lsp-core — can be added to existing handlers.

**Context file intelligence:**
- Complete file paths after `path:` in context: block
- Validate context files exist on disk (diagnostic if missing)
- Hover on context alias → preview file content (first 10 lines)
- Complete `{{context.files.ALIAS}}` from declared files
- Complete `{{context.session.KEY}}` from session keys

**Input parameter intelligence:**
- Complete `{{inputs.PARAM}}` from declared inputs
- Validate default values match declared type
- Hover on input → show type + default + usage count

---

## 6. Track C: Intelligence

> **Goal:** 10+ novel features with zero prior art.
> **Timeline:** Post Track A foundation, ~10 weeks, 9 PRs.
> **Depends on:** nika-lsp-core crate exists.

### PR:vision-lsp — Multimodal Content Intelligence

**~1 week**

The ContentPart pipeline exists (Raw→Analyzed→Lower) but LSP knows nothing about it.

**Completions:**
- After `content:` → suggest `- type: text` / `- type: image` / `- type: image_url`
- After `detail:` → `auto`, `low`, `high`
- After `source:` → CAS hash refs (from workspace media artifacts)
- After `url:` in image_url → URL placeholder

**Diagnostics:**
- `content:` on model without VISION capability → NIKA-032 (using model_intel.rs)
- Both `prompt:` and `content:` absent → error
- `detail: invalid` → error with suggestions
- `source:` with non-existent CAS hash → warning

**Hover:**
- Hover on `type: image` → documentation + example
- Hover on `detail: high` → explain cost/quality trade-off
- Hover on CAS hash → show dimensions, format, size (if file accessible)

**Inlay hints:**
- After content: block → "[2 images, ~384KB, detail: high]"

---

### PR:media-tools-lsp — 18 nika:* Tool Intelligence

**~1 week**

Replace hardcoded 12 NovaNet tools with complete builtin tool catalog.

**Tool registry in nika-lsp-core:**
```rust
pub struct BuiltinToolDef {
    pub name: &'static str,        // "nika:thumbnail"
    pub description: &'static str,
    pub tier: ToolTier,            // Tier1/Tier2/Tier3
    pub feature_gate: Option<&'static str>,  // "media-phash"
    pub params: &'static [ToolParam],
    pub returns: &'static str,
}
```

**Completions:**
- After `tool: nika:` → list all 18 tools with descriptions and tier badges
- After `params:` (per tool) → suggest required/optional params with types
- Feature gate awareness: `nika:phash` grayed out if `media-phash` not enabled

**Hover:**
- Hover on `nika:thumbnail` → full documentation + params + example
- Hover on `nika:qr_validate` → QR decode docs + scan score explanation

**Diagnostics:**
- Unknown tool name → fuzzy match suggestion
- Missing required param → quick fix to add
- Tool needs feature gate → info diagnostic with enable instruction

---

### PR:template-intelligence — 27 Transforms + Scope Lens + Type-Checking

**~2 weeks**

**Transform completions after `|`:**
- List all 31 transforms with descriptions
- Parameterized: `first(` → suggest N, `join(` → suggest separator, `round(` → suggest decimals
- Type-aware filtering: string value → show string transforms, array → show collection transforms

**Transform type-checking:**
- `{{with.name | sort}}` → ERROR: sort expects array, got string
- `{{with.items | upper}}` → ERROR: upper expects string, got array
- `{{with.data | sort | first | upper}}` → valid chain (array → element → string)

**Scope lens (what can I access here?):**
- At any cursor in a prompt, show available bindings:
  - `with.*` → all aliases declared in current task
  - `inputs.*` → all workflow input parameters
  - `context.files.*` → all context file aliases
  - `context.session.*` → all session keys
  - `item.*` → loop variable (if in for_each)
- Code lens: "Available: with.data, with.config, inputs.locale" above prompt

**Default operator (`??`) intelligence:**
- Complete default values based on inferred type
- Validate: `$task.field ?? "default"` — string default for inferred string field

---

### PR:prompt-linter — World's First In-Editor Prompt Linter

**~1 week**

**Diagnostics (severity: info/hint, not error):**
- `extended_thinking: true` on non-Claude model → ERROR
- `extended_thinking: true` on budget tier → WARNING (expensive)
- Redundant "Return JSON" in prompt when `output.format: json` → INFO
- Prompt > 2000 tokens → SUGGESTION: "Consider extracting to context file"
- `{{with.user_input}}` in prompt without sanitization → WARNING (injection risk)
- Missing system prompt for complex task → SUGGESTION
- Temperature > 1.5 → WARNING (high randomness)

**Token budget tracker (inlay hint):**
- After prompt: `|` block → "[~847 tokens / 200K context]"
- Warn when prompt + expected output > 80% of context window
- Uses char/4 approximation (fast, offline, good enough for hints)

---

### PR:cost-radar — Per-Task Cost + Model Alternatives

**~1 week**

Uses existing model_intel.rs + cost.rs. No new infrastructure needed.

**Inlay hints (OFF by default):**
- After model: field → "[~$0.03/call]" or "[$3/1M in, $15/1M out]"
- After tasks: → workflow total "[est. $0.35-$1.20]"
- After for_each: → "[× N items]" multiplier

**Code actions:**
- "Switch to haiku: save 80%" → uses `ModelCatalog::alternatives_for()`
- "Switch to gpt-4o-mini: save 90%" → cross-provider alternatives
- Shows capability delta: "loses extended_thinking, keeps vision+tools"

**Model compatibility matrix (hover):**
- Hover on model name → `ModelInfo::hover_markdown()` (already implemented!)
- Shows: pricing, capabilities, context window, lifecycle, alternatives

**Cost simulation command:**
- `nika.estimateCost` → per-task breakdown + total with confidence interval

---

### PR:cross-file — Include/Import Resolution + Workspace Diagnostics

**~2 weeks**

**Include resolution:**
- `include: ./lib/seo.nika.yaml` → Ctrl+Click opens file
- Completions: task IDs from included files in depends_on:
- Rename propagates through includes
- Diagnostic: included file not found

**Import prefix intelligence:**
- `imports: [{path: ./lib.yaml, prefix: seo_}]` → complete `seo_*` task IDs
- Go-to-def traverses imports with prefix stripping
- Diagnostic: duplicate prefix (already in analyzer!)

**Workspace diagnostics:**
- Scan ALL .nika.yaml on startup
- Report: broken includes, unused tasks, version conflicts
- LSP 3.17 diagnostic pull model (`textDocument/diagnostic`)

**Package URI resolution:**
- `pkg://supernovae/seo-tools@1.2` → validate package exists
- Auto-complete available workflows from package
- Hover: show package description + version

---

### PR:structured-output-lsp — Schema Intelligence

**~1 week**

**JSON Schema validation:**
- Validate `output.schema` syntax inline
- Hover on schema → formatted preview
- Diagnostic: invalid JSON Schema → specific error

**Downstream field completions:**
- If task A has `output.schema: { properties: { title: ..., author: ... } }`
- Then task B's `{{with.data.FIELD}}` → complete `title`, `author`
- Type-aware: string field → string transforms, number → numeric transforms

**Code actions:**
- "Generate schema from example JSON" → paste JSON, get schema
- "Validate output against schema" → dry-run validation

---

### PR:foreach-agent-retry-lsp — Modifier Intelligence

**~1 week**

**For-each intelligence:**
- Complete `{{item.field}}` when `as: item` defined
- Validate `as:` doesn't shadow task name
- Inlay hint: concurrency recommendation
- Diagnostic: for_each without `as:` → warning

**Agent loop intelligence:**
- Validate `tools:` references (builtins + MCP tools exist)
- Warn: max_turns > 20 → "expensive"
- Warn: extended_thinking + agent → "each turn costs thinking tokens"
- Inlay hint: worst-case cost = max_turns × model_cost

**Retry intelligence:**
- Inlay hint: "worst case: 3 retries × 2.0 backoff = 7s + 4× cost"
- Hover: retry timeline visualization

**Decompose intelligence:**
- Inlay hint: "runtime DAG expansion"
- Warn: unbounded decompose (no max_tasks)

---

### PR:guardrails-lsp — Guardrail Intelligence (NEW — was missing from v1 plans)

**~1 week**

The guardrails system (2,067 LOC, 100+ tests in `src/ast/guardrails.rs`) is completely absent from v1 plans despite being central to agent reliability.

**4 guardrail types:** length, schema, regex, LLM (secondary judge call)
**3 escalation modes:** retry, escalate, fail

**Completions:**
- After `guardrails:` → suggest `- type: length/schema/regex/llm`
- Per-type params: length (min/max words/chars), schema (JSON Schema), regex (pattern + negate), llm (judge_prompt + model)
- Escalation mode: `on_fail: retry/escalate/fail`

**Diagnostics:**
- Invalid JSON Schema in schema guardrail
- Invalid regex pattern in regex/llm guardrails
- LLM guardrail without judge_prompt → error
- Multiple `on_fail: fail` guardrails → warning (first failure kills the chain)

**Hover:** Show guardrail parameters, examples, escalation behavior

**Code actions:** "Add guardrail" → generate template per type

**Inlay hints:** After `guardrails:` → "[3 guardrails, max retries: 3]"

---

### PR:security-diagnostics — exec: Injection Warnings

**~3 days**

- Highlight blocked commands from security.rs blocklist (20+ patterns)
- `{{with.user_input}}` in exec: command → WARNING (injection risk)
- `shell: true` without explicit blocklist → INFO
- exec: in untrusted workspace → Workspace Trust gating

---

## 7. Track D: Magic

> **Goal:** Visual DAG, debugger, notebook — the "Figma for AI workflows" moment.
> **Timeline:** Post Track C, ~8 weeks, 6 PRs.

### PR:dag-visualization — React Flow Webview + Depth Gutter

**~3 weeks**

**VS Code webview** with React Flow v12 + ELK.js Sugiyama layout.

**Custom nodes per verb:**
- 🟣 purple = infer, 🟢 green = exec, 🔵 blue = fetch, 🟡 yellow = invoke, 🔴 red = agent

**Edges:**
- Solid = depends_on (explicit), Dashed animated = data flow (with:)
- Critical path glow (thicker, brighter)

**Bidirectional sync (Mode 2):**
- Drag node → generate YAML task block
- Edit YAML → graph updates in real-time
- Reconciliation engine handles conflicts

**Depth gutter (in editor):**
- Left margin shows DAG depth: ①②③④
- Color gradient: shallow=light, deep=dark
- Click → highlight all tasks at same depth

**Custom LSP request:** `nika/dagGraph` → returns DAG as JSON.

---

### PR:playground — Single-Task Execution

**~2 weeks**

Architecture from the [Playground REPL Design](./2026-03-18-lsp-playground-repl-design.md) (absorbed into this plan).

**PlaygroundRunner** — in-process, shares provider cache, zero cold start.

**Custom LSP requests:**
- `nika/playground/run` — run single task (real or mock)
- `nika/playground/stream` — real-time events
- `nika/playground/result` — final result + metadata
- `nika/playground/scratch` — freeform prompt (no workflow)
- `nika/playground/cancel` — abort
- `nika/playground/history` — last 10 runs per task

**Mock mode:** Echo / Fixed per verb / Sidecar .mock.json / Replay from cache.

**Security:** exec: requires confirmation dialog. Cost cap per session ($1.00 default).

---

### PR:notebook — VS Code Notebook API

**~1 week**

Cells = Tasks. Run button per cell. Output block below.

**Components:**
- `NikaNotebookSerializer` — .nika.yaml ↔ notebook cells
- `NikaNotebookController` — Kernel: ▶ Run sends nika/playground/run
- `NikaOutputRenderer` — Markdown, JSON tree, image preview
- `NikaScratchPad` — Free-form prompt testing panel

---

### PR:runtime-feedback — Trace Integration + Complexity + Diff

**~1 week**

**Execution history (if .nika-trace.ndjson exists):**
- Inlay hint after each task: "[last: 2.3s, ✓]" or "[failed: timeout]"
- Code lens: "Last run: 12.3s | $0.45 | 47 tokens"
- Hover on task → last output preview (first 200 chars)
- Gutter icon: ✅ / ❌ / ⏸️

**Workflow complexity score (inlay hint at `schema:`):**
- "[8 tasks │ depth 4 │ ~$0.85 │ ~15s]"
- Verb weights: exec=1s, fetch=2s, infer=5s, agent=5s×max_turns

**Semantic diff on save (code lens):**
- "[+1 task, -1 dep, critical path: unchanged]"
- "Switched opus→sonnet: -80% cost"

---

### PR:live-mcp — Real MCP Schema Discovery

**~1 week**

Connect to MCP servers and fetch REAL tool schemas.

**Architecture:**
- Async schema fetch at workspace open (non-blocking)
- Cache in `WorldDatabase.mcp_tool_cache`
- Refresh on mcp: config change
- Fallback to static catalog if server unreachable

**Completions:**
- `invoke: { tool: <real-tool> }` from live schemas
- `params:` from actual tool input schema
- Hover on tool → server description + example

---

### PR:recipes — Workflow Template System

**~3 days**

31+ workflow recipes (6 tiers already implemented in `src/init/tier1-6.rs`).

**Code action:** "🧩 Create from recipe" → VS Code QuickPick:
- SEO Analysis Pipeline (5 tasks)
- Content Generation (3 tasks)
- Data ETL (fetch→transform→store)
- Multi-Provider Compare (3 parallel infer + merge)
- QR Code Pipeline (import→validate→optimize→publish)
- API Integration (fetch→parse→store)
- Research Assistant (search→synthesize→format)
- Translation Pipeline (detect→translate→review)
- Image Processing (import→thumbnail→optimize→publish)
- Chatbot (agent loop with tools)
- Code Review (fetch PR→analyze→report)
- Monitoring (fetch→check→alert)

**CLI:** `nika recipe list` / `nika recipe create seo-analysis`

---

## 8. Track E: Ecosystem

> **Goal:** Long-term differentiators.
> **Timeline:** Ongoing, v0.35+.
> **7 PRs**, independently schedulable.

### PR:gradual-types — Optional Type System

`types:` block, `returns:` annotation, Phase 2.5 type checker, NIKA-155-159, cross-file types.

### PR:prompt-diff — Semantic Diff + A/B Testing

Semantic diff of prompt changes, `# @experiment:` annotations, variant comparison.

### PR:formatting — Opinionated nika fmt

Canonical key ordering, 2-space indent, comment preservation. Like Prisma.

### PR:package-registry — pkg:// URI Support

Validate packages, auto-complete workflows, show README on hover, version resolution.

### PR:skill-intelligence — Skill/Template System

Auto-complete skill references, validate parameters, hover documentation.

### PR:learning-mode — Ghost Text + Progressive Hover

- Ghost text walkthroughs for new syntax (show example while typing)
- Progressive hover docs: 3 tiers (basic → intermediate → advanced)
- "What's next?" code action (suggest next step based on DAG context)
- First-run detection: show VS Code walkthrough page
- Onboarding: "Welcome to Nika" with guided workflow creation

### PR:tui-studio — TUI IDE Layout Refactor

File tree, context inspector (4 modes), DAG panel, trace timeline, F5 Run / F6 Validate.

---

## 9. Novel Features Catalog

30 features, ranked by prior art and impact.

| # | Feature | Track | Prior Art | Impact |
|---|---------|-------|-----------|--------|
| 1 | Data Flow Type Propagation | C:structured-output | Zero | Catches deep property typos |
| 2 | Template Scope Lens | C:template | Zero | "What can I access here?" |
| 3 | Transform Chain Type-Checker | C:template | Zero | Only valid transforms suggested |
| 4 | Binding Cycle Detector | A:foundation | Zero | Catches deadlocks at edit time |
| 5 | Prompt Quality Linter | C:prompt-linter | Zero | World's first in-editor |
| 6 | Cost Radar | C:cost-radar | Infracost (IaC) | Per-task `[~$0.03]` |
| 7 | DAG Bottleneck Detection | D:dag | Zero | Critical path at edit time |
| 8 | Smart Scaffold | D:recipes | npm init | DAG-aware task generation |
| 9 | MCP Tool Signatures | D:live-mcp | Zero | IntelliSense for MCP |
| 10 | Provider Switcher | C:cost-radar | Zero | "Switch to mini: save 90%" |
| 11 | Workflow Recipes | D:recipes | npm init | 12+ templates |
| 12 | DAG Visual Editor | D:dag | Node-RED | Bidirectional YAML↔graph |
| 13 | Time-Travel Debugging | D:playground | Redux DevTools | Step through execution |
| 14 | Notebook Mode | D:notebook | Jupyter | Full notebook with cells |
| 15 | Learning Mode | E:future | Zero | Ghost text + progressive hover |
| 16 | Gradual Types | E:types | TypeScript | Optional type annotations |
| 17 | Prompt Diff | E:prompt-diff | Zero | Semantic diff |
| 18 | A/B Testing | E:prompt-diff | Zero | @experiment variants |
| 19 | Workflow Diff Preview | D:runtime-feedback | Zero | Semantic DAG diffing |
| 20 | Model Intelligence | C:cost-radar | Zero | 1,413 LOC, 65 tests (DONE) |
| 21 | Vision Content Intelligence | C:vision-lsp | Zero | LSP for multimodal |
| 22 | Media Tool Completions | C:media-tools | Zero | 18 tools with schemas |
| 23 | Cross-File Intelligence | C:cross-file | rust-analyzer | Include/import resolution |
| 24 | Security Diagnostics | C:security | Zero | exec: injection warnings |
| 25 | Token Budget Tracker | C:prompt-linter | Zero | Live token count |
| 26 | Complexity Score | D:runtime-feedback | SonarQube | Workflow health metric |
| 27 | Execution History Overlay | D:runtime-feedback | Zero | Trace file integration |
| 28 | Live MCP Discovery | D:live-mcp | Zero | Real schemas from servers |
| 29 | Depth Gutter | D:dag | Zero | Visual DAG depth in editor |
| 30 | Agent Turn Cost Warning | C:foreach-agent | Zero | "max_turns=50 → ~$7.50" |

---

## 10. Error Code Coverage Map

~102 unique error codes across 25 ranges (verified against `error.rs`, `analyzer/errors.rs`, `parser.rs`, `media/error.rs`).

Current LSP coverage: **~0%** — The 13 codes from AnalyzeErrorKind (NIKA-140-151) and ParseErrorKind (NIKA-160-164) exist in the analyzer but are **not yet wired to LSP diagnostic publishing** in v0.34.0. They are available for surfacing but require the diagnostics pipeline in PR:handler-migration-wire.

**Known bug:** NIKA-160/161 collision — `ParseErrorKind::Syntax/MissingField` and `NikaError::PolicyViolation/BootFailed` both use NIKA-160/161. **Fix:** Reassign ParseErrorKind to NIKA-155-159 range in PR:extract-ast.

**Terminology:**
- **Surfaced** = appears as LSP diagnostic squiggly line
- **Quick Fix** = has a code action to fix it
- **Runtime-only** = only occurs during execution, not at edit time (cannot be surfaced)

### Complete Coverage Map

| Range | Category | Codes | Current | After B | After C | Surfaceable? |
|-------|----------|:-----:|:-------:|:-------:|:-------:|:------------:|
| 000-009 | Workflow (parse, schema, not found) | 6 | ❌ | ✅ | ✅ | Yes |
| 010-019 | Schema validation | 2 | ❌ | ✅ | ✅ | Yes |
| 020-029 | DAG (cycles, deps, cancelled) | 7 | ❌ | ✅ | ✅ | Yes |
| 030-039 | Provider (config, API, auth) | 4 | ❌ | ✅ | ✅ | Partial |
| 040-049 | Template/binding errors | 4 | ❌ | Partial | ✅ | Yes |
| 050-059 | Task/path/security | 5 | ❌ | ✅ | ✅ | Yes |
| 060-069 | Output (JSON, schema) | 3 | ❌ | ❌ | ✅ | Yes |
| 070-079 | With block validation | 4 | ❌ | ✅ | ✅ | Yes |
| 080-089 | DAG with: validation | 3 | ❌ | ✅ | ✅ | Yes |
| 090-099 | JSONPath/IO/execution | 4 | ❌ | ❌ | Partial | Partial |
| 100-109 | MCP (connection, tools, timeout) | 10 | ❌ | ❌ | ✅ | Partial |
| 110-119 | Agent (validation, execution) | 3 | ❌ | ❌ | Partial | Partial |
| 120-129 | Resilience (timeout, retry) | 2 | ❌ | ❌ | ❌ | Runtime-only |
| 130-139 | TUI/Config | 2 | ❌ | ❌ | ❌ | Runtime-only |
| **140-151** | **AST analysis (Phase 2)** | **8** | **✅** | **✅** | **✅** | **Yes** |
| 150-159 | Startup (policy, boot) | 2 | ❌ | ❌ | ❌ | Runtime-only |
| **160-164** | **Phase 1 parse errors** | **5** | **✅** | **✅** | **✅** | **Yes** |
| 160-169 | Policy/Boot (separate from parse) | 2 | ❌ | ❌ | ❌ | Runtime-only |
| 170-179 | Runtime (decompose) | 1 | ❌ | ❌ | ❌ | Runtime-only |
| 200-219 | File tools + builtin tools | ~15 | ❌ | ❌ | ❌ | Runtime-only |
| 250 | Context error | 1 | ❌ | ❌ | ✅ | Yes |
| 251-259 | Media pipeline | 9 | ❌ | ❌ | Partial | Runtime-only |
| 260-269 | Package URI errors | 2 | ❌ | ❌ | ❌ | Yes (Track E) |
| 270-279 | Skill errors | 1 | ❌ | ❌ | ❌ | Yes (Track E) |
| 280-289 | Artifact/media errors | 6 | ❌ | ❌ | ❌ | Runtime-only |
| 290-297 | Media tool errors | 8 | ❌ | ❌ | ❌ | Runtime-only |
| 300-309 | Structured output | 4 | ❌ | ❌ | ✅ | Partial |

**Note on NIKA-160-164:** Phase 1 parse errors use `ParseErrorKind` (Syntax, MissingField, InvalidType, UnknownField, InvalidSchema) which are distinct from NIKA-160/161 PolicyViolation/BootFailed in the NikaError enum. Both exist; the Phase 1 parse errors are the ones surfaced in LSP.

**Surfaceable codes (3 tiers):**
- **13 codes** provably detectable at edit time today (AST analysis + parse errors)
- **~40 codes** surfaceable with expanded static analysis (schema, DAG, binding, MCP config validation)
- **~50 codes** runtime-only (provider API errors, execution failures, MCP connection issues) — never surfaceable

**Targets (% of surfaceable ~40 codes):** Track B: 0% → 50% | Track C: 50% → 80% | Track D+: 80% → 95%

---

## 11. Testing Strategy

### Baseline (v0.34.0)

| Category | Current | Source |
|----------|---------|--------|
| Total tests | 8,031 | `cargo test --lib` |
| LSP embedded tests | 210 | `src/lsp/**/*.rs` |
| LSP standalone tests | 73 | `nika-lsp/src/**/*.rs` |
| AST tests | 733 | `src/ast/**/*.rs` |
| Proptest strategies | 7 `proptest!` blocks (34 test fns) | `tests/proptest_fuzzing.rs` |
| Insta snapshots | 28 files | `tests/regression/snapshots/` |
| Criterion benchmarks | 0 active | `tests/benchmarks/micro_benchmarks.rs` (dormant) |
| Broken YAML fixtures | 9 | `tests/regression/snapshots/` |
| Integration suites | 70+ files | `tests/` |

### Target by Track

| Category | After A | After B | After C | After D |
|----------|---------|---------|---------|---------|
| LSP tests (core) | 400+ | 400+ | 600+ | 700+ |
| Broken YAML fixtures | 50+ | 50+ | 50+ | 50+ |
| Criterion benchmarks | 5+ | 10+ | 10+ | 15+ |
| Proptest strategies | 10+ | 10+ | 12+ | 12+ |
| Insta snapshots | 100+ | 100+ | 150+ | 200+ |
| Total tests | 8,500+ | 8,500+ | 9,000+ | 9,500+ |

### Test Pyramid

```
                    /\
                   /  \      Integration: 20+ (real LSP protocol)
                  /────\
                 /      \    Snapshot: 100+ (insta YAML per handler)
                /────────\
               /          \  Property: 30+ (proptest for positions)
              /────────────\
             /              \ Benchmark: 10+ (criterion for latency)
            /────────────────\
           /                  \ Unit: 400+ (pure function tests)
          /____________________\
```

### Test Commands

```bash
cargo test -p nika-lsp-core --lib     # Core unit tests (safe, no keychain)
cargo test -p nika-lsp-core            # + integration
cargo insta test -p nika-lsp-core --review  # Snapshot updates
cargo bench --bench micro_benchmarks   # Performance benchmarks
```

---

## 12. VS Code Extension Roadmap

### Current State: v0.1.0 (score 2/10)

242 LOC, 6 files. LSP client + grammar. No snippets, commands, icons, keybindings.

### After Track A: v0.31.0-alpha (score 3/10)

- All LSP intelligence in nika-lsp-core (no extension changes yet)
- Extension still uses thin shim, but handlers are faster + richer

### After Track A+B: v0.31.0 (score 6/10)

- 7 snippets, 4 commands, 2 keybindings, status bar, 2 icons
- 10 settings, grammar fixes (multiline, content:, edges:)
- TextMate scope coverage: 95%+

### After Track C: v0.32.0 (score 8/10)

- All intelligence features visible (inlay hints, cost radar, prompt linter)
- Full error coverage (65% of surfaceable codes)
- Vision + media tool completions

### After Track D: v0.33.0 (score 10/10)

- React Flow DAG webview (bidirectional Mode 2)
- Playground panel (Run/Mock per task)
- Notebook serializer + controller + renderers
- Scratch pad panel, trace replay panel
- 30+ extension tests

### After Track E: v0.34.0+ (score 11/10)

- Formatting (nika fmt)
- Package registry integration
- Learning mode: ghost text walkthroughs, progressive hover (3 tiers), "What's next?" code action, first-run walkthrough page
- Skill/template intelligence
- Theme contributions (verb colors)

---

## 13. Performance Targets

| Metric | Current | Target | How to Measure |
|--------|---------|--------|----------------|
| Completion latency | ~200ms (reparse) | < 50ms | criterion bench, 100-task fixture |
| Hover latency | ~100ms | < 30ms | criterion bench |
| Diagnostic latency | ~200ms (blocking) | < 200ms (non-blocking) | debounced, wall-clock |
| Position conversion | O(n) per call | O(log n) | criterion, proptest roundtrip |
| Node lookup | O(n) linear | O(log n) | criterion, PositionIndex |
| Startup | ~1s | < 1s | wall-clock to first diagnostic |
| Memory (100 tasks) | ~10MB | < 5MB | RSS measurement |
| Broken YAML completions | 0% | 100% | fixture corpus |
| Edit response (10 keystrokes) | 10 reparses | 1 reparse | debounced scheduler |

---

## 14. UX Philosophy

### Trust Equation

```
Trust = (Accuracy × Relevance) / (Frequency × Intrusiveness)
```

One false positive in a 20-line workflow destroys trust for the entire LSP.

### 3 Diagnostic Levels

| Level | Default | Shows |
|-------|---------|-------|
| `essential` | **YES** (new users) | Errors only |
| `recommended` | After opt-in | + Warnings |
| `comprehensive` | Power users | + Info/hints/best practices |

### Inlay Hint Defaults

All individually toggleable in VS Code settings.
- **ON** (3): dependency chain, timeout clarification, binding source
- **OFF** (6): verb badge, template preview, provider, cost estimate, task count, for_each count

**Decision:** Task count defaults to OFF (reversed from v1 draft). Rationale: noise for small workflows (<10 tasks), only useful for 50+ tasks. Users opt in via `nika.inlayHints.taskCount.enabled`.

---

## 15. Security Model

**Execution security:**
- Code Lens "Run" gated by VS Code **Workspace Trust**
- Untrusted workspaces: "Validate" only, never "Run"
- `exec:` tasks require confirmation in playground mode
- Per-session cost cap in Playground mode ($1.00 default)

**Template security:**
- Template injection prevention: resolved values never re-evaluated
- `{{with.user_input}}` in exec: → WARNING diagnostic (injection risk)

**File path security (LSP-specific):**
- `include:` paths validated against traversal attacks (no `../../etc/passwd`)
- Uses same `validate_import_path()` as media import pipeline
- Document Link handler MUST validate paths before creating clickable links
- `pkg://` URIs resolved through registry client, never direct file access

**MCP connection security (PR:live-mcp):**
- MCP server URIs validated (no localhost SSRF unless explicitly configured)
- Schema fetch timeouts (5s max per server)
- Cache invalidation on config change (stale schemas = stale completions)
- MCP connections only to servers declared in workflow `mcp:` block
- No auto-discovery of undeclared servers

---

## 16. Accessibility

**Diagnostics:**
- Severity prefix in ALL messages: `[ERROR] NIKA-020: ...` (no color dependency)
- Three-sentence diagnostic: what is wrong, why it matters, how to fix it
- Tooltips on all inlay hints with full descriptive sentences

**Navigation:**
- Hierarchical document symbols for keyboard navigation
- Call hierarchy for DAG traversal without mouse
- Code lens keyboard-accessible in all LSP clients (not just VS Code)

**Visual:**
- Test semantic tokens against high-contrast themes
- DAG webview (PR:dag-visualization) MUST provide text-based fallback for screen readers
- React Flow nodes need ARIA labels with task ID + verb + status

**Streaming output:**
- Playground streaming results need ARIA live regions for screen reader announcements
- Progress indicator uses `WorkDoneProgress` (standard LSP, accessible by default)

---

## 17. Code Review Methodology

### Per-PR Checklist

**Pre-PR:**
- [ ] Read existing related code thoroughly
- [ ] Write handler test signatures FIRST (TDD)
- [ ] Property-based tests for position-dependent features (proptest)

**During PR:**
- [ ] `cargo test -p nika-lsp-core --lib` passes
- [ ] `cargo clippy -- -D warnings` zero warnings
- [ ] No new dependencies without justification
- [ ] Error codes follow NIKA-XXX convention
- [ ] NikaError, not anyhow
- [ ] 1 FIX = 1 COMMIT with co-author lines

**Post-PR:**
- [ ] Architecture conformance (3-crate boundaries)
- [ ] Performance check (criterion for new hot paths)
- [ ] Security review (exec:, MCP connections, file paths)
- [ ] No regressions in 8,000+ tests

---

## 18. Archives

Previous plans superseded by this document:

| File | Date | Status |
|------|------|--------|
| `2026-03-17-ast-lsp-quality-sweep.md` | Mar 17 | **ARCHIVED** — 16/16 tasks DONE |
| `2026-03-18-nika-lsp-definitive-plan.md` | Mar 18 | **SUPERSEDED** by this plan |
| `2026-03-18-lsp-11-out-of-10-master-plan.md` | Mar 18 | **SUPERSEDED** by this plan |
| `2026-03-18-lsp-playground-repl-design.md` | Mar 18 | **ABSORBED** into Track D |
| `research-lsp-parsing-infrastructure.md` | Mar 18 | Reference only (still valid) |
| `30-lsp-gold-standard-research.md` | Mar 18 | Reference only (still valid) |
| `30-lsp-innovations-research.md` | Mar 18 | Reference only (still valid) |
| `31-lsp-world-class-ux-vision.md` | Mar 18 | Reference only (still valid) |
| `32-lsp-ux-socratic-deep-dive.md` | Mar 18 | Reference only (still valid) |

---

## Grand Summary

| Metric | Today | After A | After A+B | After C | After D | After E |
|--------|-------|---------|-----------|---------|---------|---------|
| Standard LSP features | 7/24 | 21/24 | 21/24 | 23/24 | 24/24 | 24/24 |
| Code duplication | 65% | 0% | 0% | 0% | 0% | 0% |
| Error surfacing (of ~40 surfaceable) | 0%* | 0%* | 50% | 80% | 95% | 95% |
| Completion latency | ~200ms | <50ms | <50ms | <50ms | <50ms | <50ms |
| Broken YAML support | 0% | 100% | 100% | 100% | 100% | 100% |
| Novel features | 1 | 1 | 2 | 16 | 28 | 31 |
| Tests | 8,031 | 8,500+ | 8,700+ | 9,000+ | 9,500+ | 9,500+ |
| VS Code score | 2/10 | 3/10 | 6/10 | 8/10 | 10/10 | 11/10 |

*Track A builds the diagnostics pipeline infrastructure (WorldDatabase, error recovery, handler migration). Track B wires the actual error codes through it with quick fixes.

**Total: 5 tracks, 34 named PRs, 31 novel features, ~102 error codes.**

**The result:** The first YAML workflow LSP that approaches rust-analyzer quality — AND has 30 features no LSP has ever had, because no other tool is simultaneously a language, a runtime, an AI orchestrator, and an MCP integration platform.
