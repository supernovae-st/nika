# Nika LSP — 11/10 Master Plan

> Date: 2026-03-18 (enriched: same day, round 3)
> Research: 20 agents across 3 rounds — Perplexity, rust-architect x3, rust-perf, code audit x3, error mapping, VS Code analysis, web research x3, Socratic UX deep-dive
> Goal: The best workflow-DSL LSP in existence. Period.

---

## Executive Summary

The Nika LSP is currently 6.2/10 — functional but fragmented (two implementations, 65% code duplication, only 32% of error codes surfaced). This plan transforms it into 11/10 through:

1. **Architecture consolidation** — 3-crate design eliminating all duplication
2. **Error recovery** — tree-sitter-yaml CST so everything works on broken code
3. **Performance** — generation-based caching, debouncing, O(log n) lookups
4. **Standard LSP features** — inlay hints, code lens, rename, references, folding
5. **10 novel Nika-only features** — cost radar, prompt linter, data flow types, DAG bottleneck detection, smart scaffold (things NO other LSP has)
6. **VS Code extension overhaul** — from v0.1 skeleton to production-grade

**Realistic timeline: v0.31 in ~9 weeks (6 PRs), v0.32+ for advanced features.**

### The Unique Insight

Nika occupies a position NO other tool occupies: it is simultaneously a **language** (YAML DSL), a **runtime** (DAG executor), an **AI orchestrator** (LLM inference), and an **integration platform** (MCP). Every existing LSP covers at most one of these. This creates feature opportunities with zero prior art.

---

## Table of Contents

1. [Architecture: The 3-Crate Design](#phase-0-architecture-the-3-crate-design)
2. [Critical Risks & Mitigations](#critical-risks--mitigations)
3. [Phase 1: Foundation (PRs 1-3)](#phase-1-foundation-prs-1-3)
4. [Phase 2: Core UX (PRs 4-6)](#phase-2-core-ux-prs-4-6)
5. [Phase 3: Intelligence (PRs 7-8)](#phase-3-intelligence-prs-7-8)
6. [Phase 4: Magic (PRs 9-10)](#phase-4-magic-prs-9-10)
7. [10 Novel Nika-Only Features](#10-novel-nika-only-features)
8. [Error Code Coverage Map](#error-code-coverage-map)
7. [VS Code Extension Overhaul](#vs-code-extension-overhaul)
8. [Performance Targets](#performance-targets)
9. [Testing Strategy](#testing-strategy)
10. [Architectural Decision Records](#architectural-decision-records)

---

## Critical Risks & Mitigations (from brutal critique)

### Risk 1: tree-sitter-yaml implements YAML 1.1, Nika uses YAML 1.2

**Impact:** YAML 1.1 treats `yes`/`no`/`on`/`off` as booleans. YAML 1.2 does not. A user writing `output: yes` gets different CST node types from tree-sitter vs marked_yaml.

**Mitigation (MANDATORY):** The bridge layer (`bridge.rs`) ONLY extracts structural information (key positions, indentation, block structure). It NEVER trusts tree-sitter's value interpretation. All value semantics come from marked_yaml. This is an architectural invariant.

### Risk 2: Inter-file dependency tracking missing from WorldDatabase

**Impact:** File A includes File B. User edits File B. File A's cache is "fresh" (its text didn't change) but its analysis is stale.

**Mitigation:** Add `include_dependencies: DashMap<Url, Vec<Url>>` and `workspace_revision: AtomicU64` to WorldDatabase. Multi-file queries check workspace_revision, not per-file generation. Single-file queries still use per-file generation.

### Risk 3: tower-lsp 0.20 is abandoned

**Impact:** Known cancellation issues. The community fork is `tower-lsp-server` 0.23 with Rust 2024 edition support.

**Mitigation:** PR 0 (before consolidation): upgrade to `tower-lsp-server 0.23`. Or at minimum, acknowledge this as a Phase 2 dedicated PR.

### Risk 4: Scope is ~6 months, not 15 weeks

**Impact:** The plan as originally written has 10 PRs with 30+ features. That's not 15 weeks.

**Mitigation:** **v0.31 is 6 PRs in ~9 weeks.** Everything else is v0.32+. See revised roadmap below.

### Risk 5: nika-lsp-core depending on full `nika` crate

**Impact:** As nika grows (native inference, daemon, MCP client), this coupling becomes a compile-time and binary-size problem.

**Mitigation:** Long-term, extract `nika-ast` and `nika-error` as separate crates. Short-term, acceptable.

### Risk 6: Standalone `nika-lsp` binary becomes a maintenance ghost

**Decision:** Keep both. `nika lsp` = integrated (feature-gated, for users who already have nika). `nika-lsp` = standalone (lighter install, for VS Code users who only want editor support). Document this explicitly.

---

## UX Philosophy (from Socratic research)

### The Trust Equation

```
Trust = (Accuracy x Relevance) / (Frequency x Intrusiveness)
```

- One false positive in a 20-line workflow destroys trust for the entire LSP
- Features must be accurate before they are numerous

### The Dangerous Middle

Most LSPs fail in the "dangerous middle" — they have good features AND noisy features, and users can't distinguish them. Trust decays for the whole system.

**Solution:** Radical transparency about confidence levels.

### Three Diagnostic Levels

| Level | What's shown | Default for |
|-------|-------------|-------------|
| `essential` | Errors only (parse failures, type mismatches, cycles) | **DEFAULT** (new users) |
| `recommended` | + Warnings (unused tasks, missing deps, prompt issues) | After 1 week of use |
| `comprehensive` | + Info/hints (best practices, cost, parallelism suggestions) | Power users |

**Rationale:** "Default to essential because Nika is new and first impressions are irreversible. A new user who sees 5 suggestions alongside 2 real errors will not learn to distinguish them." (from rust-analyzer creator matklad's explicit guidance: start conservative, expand later)

### Inlay Hint Defaults (revised after critique)

| Hint | Default | Rationale |
|------|---------|-----------|
| Dependency chain | **ON** | Deps are invisible without scrolling. High value. |
| Timeout clarification | **ON** | `30` is ambiguous (seconds? ms?). Clarifying is valuable. |
| Binding source | **ON** | Useful for beginners, acceptable for experts. |
| Verb badge | **OFF** | Verb is already visible as the YAML key. `infer:` already says "infer." |
| Template preview | **OFF** | Templates are dynamic; static preview can be misleading. |
| Provider | **OFF** | Only useful when debugging provider issues. |
| Cost estimate | **OFF** | Dynamic, potentially inaccurate. |
| Task count | **OFF** | Noise for small workflows, useful only for 50+ tasks. |
| for_each count | **OFF** | Count is often unknowable at edit time (runtime data). |

### Code Lens Security

- Gate execution behind VS Code **Workspace Trust** API
- In untrusted workspaces: show only "Validate" (read-only), never "Run"
- In trusted workspaces: show confirmation dialog before `nika run`

### Accessibility

- Severity prefix in ALL diagnostic messages: `[ERROR] NIKA-020: ...` (no color dependency)
- Always set `tooltip` on inlay hints with full descriptive sentences
- Three-sentence diagnostic pattern: what is wrong, why it matters, how to fix it
- Test semantic tokens against high-contrast themes
- Hierarchical document symbols for keyboard navigation

---

## Phase 0: Architecture — The 3-Crate Design

### Problem

Two LSP implementations with 65% code duplication:
- **Embedded** (`src/lsp/`, 8.7K LOC): tower-lsp 0.20, 179 tests, HashMap doc store
- **Standalone** (`nika-lsp/`, 4K LOC): ropey Rope, debounced validation, MCP discovery, template validation

Neither is complete. Both have `FileId(0)` hardcoded. Both fall back to text regex when spans fail.

### Solution: `nika-lsp-core` Library Crate

```
nika/tools/
  nika/                          # Main binary (unchanged)
    src/lsp.rs                   # Thin shim: re-export nika-lsp-core + run_stdio()
    Cargo.toml                   # lsp = ["dep:nika-lsp-core", "dep:tower-lsp"]

  nika-lsp-core/                 # NEW: All LSP intelligence
    src/
      lib.rs                     # Public API
      db.rs                      # WorldDatabase (generation-based incremental cache)
      document.rs                # Rope-based document management
      position.rs                # LineIndex for O(1) offset <-> Position
      protocol.rs                # Span -> LSP Range conversions
      workspace.rs               # Multi-file workspace + include: resolution
      index/
        ast_index.rs             # Per-file AST cache with version tracking
        symbol_index.rs          # Cross-file symbol table (tasks, MCP servers)
      parse/
        recovery.rs              # tree-sitter-yaml error-recovery parser
        bridge.rs                # CST -> PartialWorkflow extraction
      analysis/
        context.rs               # Cursor context detection (3-strategy)
        diagnostics.rs           # NIKA error -> LSP Diagnostic conversion
        template.rs              # {{with.*}} validation
      handlers/
        completion.rs            # Merged: schema + verb + binding + MCP + template
        hover.rs                 # Merged: verb docs + task docs + field docs
        definition.rs            # Go-to-def: tasks, bindings, includes
        references.rs            # NEW: Find all references
        rename.rs                # NEW: Rename task IDs across file
        code_action.rs           # Quick fixes + refactors (expanded)
        semantic_tokens.rs       # Syntax highlighting tokens
        symbols.rs               # Document + workspace symbols
        inlay_hints.rs           # NEW: dependency, verb, timeout, binding hints
        code_lens.rs             # NEW: Run/Validate/DAG/dependents
        folding.rs               # NEW: Smart YAML folding
        document_link.rs         # NEW: Clickable paths/URLs
        formatting.rs            # NEW: Opinionated YAML format
        signature_help.rs        # NEW: invoke: parameter hints
      knowledge/
        schema.rs                # Nika schema knowledge (verbs, fields, transforms)
        mcp_tools.rs             # MCP tool discovery (from mcp_discovery.rs)
        providers.rs             # NEW: Provider/model catalog with costs

  nika-lsp/                      # Standalone binary (thin wrapper)
    src/main.rs                  # ~30 lines: tower-lsp wiring + stdio
```

### Key Architectural Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Crate structure | 3 crates (core lib + 2 thin binaries) | Eliminates 65% duplication |
| tower-lsp dep | Only in entry points, not core | Core is testable without transport |
| Document storage | `ropey::Rope` | O(log n) edits vs O(n) String |
| Incremental cache | Generation-based WorldDatabase (not salsa) | 90% benefit at 10% complexity |
| Error recovery | tree-sitter-yaml CST + bridge | Only YAML parser with error recovery |
| Position conversion | LineIndex with binary search | O(log n) vs O(n) per lookup |
| Context detection | 3-strategy: AST > CST > line fallback | Always works, best quality possible |
| Handlers | Pure functions `fn(db, file, pos) -> Response` | Testable, no state |
| Multi-file | FileKey allocation in WorldDatabase | Replaces FileId(0) |
| lsp-types | v0.94 (match tower-lsp 0.20) | Upgrade to 0.97 later |

### WorldDatabase — The Heart

```rust
pub struct WorldDatabase {
    // Global revision counter
    revision: AtomicU64,
    // Source text per file (Arc<str> + generation)
    texts: DashMap<Url, (Arc<str>, u64)>,
    // Cached parse results (generation-gated)
    parsed: DashMap<Url, Versioned<ParsedDocument>>,
    // Cached analysis results
    analyzed: DashMap<Url, Versioned<AnalyzedSnapshot>>,
    // Pre-computed line indices for O(1) position conversion
    line_indices: DashMap<Url, Versioned<LineIndex>>,
    // Sorted span index for O(log n) node lookup
    position_index: DashMap<Url, Versioned<PositionIndex>>,
    // URI <-> FileKey bidirectional mapping
    uri_to_key: DashMap<Url, FileKey>,
    key_to_uri: DashMap<FileKey, Url>,
    // Workspace roots
    workspace_roots: RwLock<Vec<PathBuf>>,
}
```

**Query chain:** `text(uri) -gen-> parsed(uri) -gen-> analyzed(uri) -gen-> diagnostics`

Each query checks `input_generation == cached_generation`. Stale? Recompute. Fresh? Return `Arc::clone` (zero-copy).

### Error Recovery — 3-Layer Strategy

```
Document text (possibly broken YAML)
    |
    v
[tree-sitter-yaml] --> CST (ALWAYS available, incremental re-parse)
    |                    |
    |              [bridge.rs] --> PartialWorkflow (task IDs, verbs, spans)
    |                              Used for completions/hover on broken code
    |
[marked_yaml]      --> RawWorkflow (when YAML is valid)
    |
[analyzer]         --> AnalyzedWorkflow + errors (full semantics)
```

- **Layer 1**: tree-sitter CST — always produces a tree, even for `tasks:\n  -id: broken\n`
- **Layer 2**: Bridge extracts PartialWorkflow from CST — enough for completions, hover, symbols
- **Layer 3**: Full Nika pipeline when YAML is valid — precise diagnostics, bindings, templates

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
- **10 fast keystrokes**: Only the last one triggers analysis. First 9 are cancelled.

### Cursor Context Detection — 3 Strategies

```rust
pub fn detect_context(db: &WorldDatabase, file: FileKey, position: Position) -> CursorContext {
    // Strategy 1: Full AST (best quality, only when YAML is valid)
    if let Some(analyzed) = db.get_analyzed_ast(file) {
        if let Some(ctx) = context_from_analyzed_ast(&analyzed, offset) {
            return ctx;
        }
    }
    // Strategy 2: tree-sitter CST (works on broken YAML)
    if let Some(cst) = db.get_cst(file) {
        if let Some(ctx) = context_from_cst(&cst, source, offset) {
            return ctx;
        }
    }
    // Strategy 3: Line-based fallback (always works)
    context_from_lines(source, position)
}
```

Context types: `WorkflowRoot`, `TaskField`, `VerbValue`, `WithBinding`, `Template`, `Invoke`, `McpConfig`, `Provider`, `DependsOn`, `ForEach`, `StructuredOutput`, `Unknown`.

---

## Phase 1: Foundation (PRs 1-3)

### PR 1: Create `nika-lsp-core` crate with WorldDatabase

**Goal**: New crate with core infrastructure. No handlers yet. Both existing LSPs untouched.

**Files created** (12 files):
```
nika-lsp-core/
  Cargo.toml
  src/
    lib.rs
    db.rs              # WorldDatabase + Versioned<T> + generation tracking
    document.rs        # Rope-based document (from nika-lsp/document.rs)
    position.rs        # LineIndex for O(1) offset/position conversion
    protocol.rs        # Span -> Range, Position -> offset conversions
    workspace.rs       # Multi-file workspace + include resolution stub
    index/
      mod.rs
      ast_index.rs     # Per-file AST cache with FileKey (replaces FileId(0))
      symbol_index.rs  # Cross-file symbol table stub
```

**Dependencies**:
```toml
[dependencies]
nika = { path = "../nika", version = "0.30.5" }
lsp-types = "0.94"
ropey = "1.6"
dashmap = "6"
parking_lot = "0.12"
rustc-hash = "2"
tracing = "0.1"

[dev-dependencies]
pretty_assertions = "1"
insta = { version = "1.34", features = ["yaml"] }
```

**Key implementation details**:
- `LineIndex::new(text)` — O(n) once, O(log n) per lookup. Handles `\r\n` and UTF-16 surrogates.
- `WorldDatabase` — DashMap-based, generation-gated queries, Arc<str> text (no redundant copies)
- `PositionIndex::build(workflow)` — Sorted span entries for O(log n) node-at-offset lookup
- Migrate the embedded LSP's `conversion.rs` (881 LOC, 142 tests, correct surrogate handling)

**Tests**: 80+ (migrate position/conversion tests from both implementations + new WorldDatabase tests)

**Estimated LOC**: ~1,500 new

---

### PR 2: Error recovery parser (tree-sitter-yaml)

**Goal**: LSP features work on broken YAML.

**Files created** (3 files):
```
nika-lsp-core/src/parse/
  mod.rs
  recovery.rs          # tree-sitter-yaml incremental parser
  bridge.rs            # CST -> PartialWorkflow extraction
```

**New dependencies**:
```toml
tree-sitter = "0.24"
tree-sitter-yaml = "0.7"
```

**Key implementation**:
```rust
// recovery.rs
pub struct RecoveryParser {
    parser: Parser,  // tree-sitter
}

impl RecoveryParser {
    pub fn parse_incremental(&mut self, source: &[u8], old_tree: Option<&Tree>) -> Option<Tree>;
}

// bridge.rs
pub struct PartialWorkflow {
    pub schema: Option<String>,
    pub workflow_name: Option<String>,
    pub task_ids: Vec<PartialTask>,
    pub mcp_servers: Vec<String>,
    pub has_errors: bool,
}

pub fn extract_partial_workflow(tree: &Tree, source: &[u8]) -> PartialWorkflow;
```

The bridge walks tree-sitter's CST looking for known YAML keys (`schema`, `tasks`, `id`, `infer`, `exec`, etc.) and extracts structural information even when the YAML has ERROR nodes.

**Tests**: 30+ (broken YAML fixtures that still produce PartialWorkflow with task IDs)

**Estimated LOC**: ~800 new

---

### PR 3: Migrate handlers to nika-lsp-core + debouncing

**Goal**: All handler logic in core crate. Both entry points delegate. Analysis is debounced.

**Files created** (16 files):
```
nika-lsp-core/src/
  analysis/
    mod.rs
    context.rs           # 3-strategy cursor context detection
    diagnostics.rs       # NIKA error -> LSP Diagnostic (ALL error codes)
    template.rs          # {{with.*}} validation
  handlers/
    mod.rs
    completion.rs        # Merged from both implementations
    hover.rs             # Merged from both implementations
    definition.rs        # Go-to-def: tasks, bindings, includes
    code_action.rs       # Quick fixes (expanded from 4 -> 20+)
    semantic_tokens.rs   # From embedded LSP
    symbols.rs           # Document symbols (hierarchical)
    diagnostics.rs       # Diagnostic publishing logic
  knowledge/
    mod.rs
    schema.rs            # Static Nika schema knowledge
    mcp_tools.rs         # MCP tool catalog (from nika-lsp/mcp_discovery.rs)
```

**Files modified**:
- `nika/src/lsp/server.rs` — Thin delegation to nika-lsp-core handlers
- `nika-lsp/src/backend.rs` — Thin delegation to nika-lsp-core handlers

**Files deleted** (after migration verified):
```
# Embedded LSP (6 handler files + 4 infrastructure files)
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

# Standalone LSP (9 handler/utility files)
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

**Merge strategy per handler**:

| Handler | Base | Merge From | Additions |
|---------|------|------------|-----------|
| completion | Embedded (1184 LOC) | Standalone: MCP snippets, provider completions, structured output | Template completions, depends_on task suggestions |
| hover | Embedded (1007 LOC) | Standalone: enhanced verb docs | Schema constraint hints, transform docs |
| definition | Embedded (946 LOC) | — | Include path resolution (multi-file) |
| code_action | Embedded (1135 LOC) | — | Expand from 4 to 20+ fixes (see error map) |
| semantic_tokens | Embedded (1181 LOC) | — | Template expression tokens |
| symbols | Embedded (823 LOC) | — | MCP servers in outline |
| context | — | Standalone (586 LOC) | Add CST fallback strategy |
| template | — | Standalone (355 LOC) | Add transform validation |
| mcp_tools | — | Standalone (268 LOC) | Live MCP discovery stub |
| diagnostics | — | Both (partial) | ALL NIKA error codes (see error map) |

**Debouncing added to server.rs**:
```rust
// did_change: apply edit immediately, signal scheduler
// scheduler: 150ms debounce, cancel on new edit
// completion/hover: read from cache, never wait
```

**Tests**: 250+ (migrate 179 from embedded + standalone tests + new merged tests)

**Net effect**: ~5,000 LOC deleted, ~3,000 LOC in nika-lsp-core (net: -2,000 LOC, zero duplication)

---

## Phase 2: Core UX (PRs 4-6)

### PR 4: Inlay Hints

**Goal**: Make invisible workflow structure visible inline.

**File**: `nika-lsp-core/src/handlers/inlay_hints.rs`

**Hint types** (all individually configurable):

| Hint | Example | Default |
|------|---------|---------|
| **Verb badge** | `research:` ← `[infer]` | ON |
| **Dependency chain** | `research:` ← `[depends: none]` | ON |
| **Timeout** | `timeout: 30` ← `[= 30 seconds]` | ON |
| **Binding source** | `data: $research` ← `[from: research.output]` | ON |
| **Template preview** | `{{with.data}}` ← `[= research.output]` | OFF |
| **Provider** | `model: claude-sonnet-4-5-20250514` ← `[anthropic]` | OFF |
| **Cost estimate** | `model: claude-sonnet-4-5-20250514` ← `[~$3/1K]` | OFF |
| **Task count** | `tasks:` ← `[12 tasks, 3 parallel]` | ON |
| **for_each count** | `for_each: [...]` ← `[5 items, concurrency: 3]` | ON |

**VS Code settings** (added to extension):
```json
{
  "nika.inlayHints.verb.enabled": true,
  "nika.inlayHints.dependencies.enabled": true,
  "nika.inlayHints.timeout.enabled": true,
  "nika.inlayHints.binding.enabled": true,
  "nika.inlayHints.template.enabled": false,
  "nika.inlayHints.provider.enabled": false,
  "nika.inlayHints.cost.enabled": false,
  "nika.inlayHints.taskCount.enabled": true,
  "nika.inlayHints.forEach.enabled": true
}
```

**Estimated LOC**: ~500

---

### PR 5: Code Lens + Run/Validate Integration

**Goal**: One-click workflow execution from the editor.

**File**: `nika-lsp-core/src/handlers/code_lens.rs`

**Lens types**:

| Position | Lens | Command |
|----------|------|---------|
| `schema:` line | `Run Workflow` / `Validate` / `Show DAG` | `nika.run` / `nika.check` / `nika.dag` |
| Each task `id:` | `Run from here` / `N dependents` | `nika.runTask` / `nika.showDependents` |
| `mcp:` section | `Test connections` | `nika.testMcp` |

**VS Code extension changes** (register commands):
```typescript
// Commands to register
vscode.commands.registerCommand('nika.run', (uri) => { /* spawn nika run */ });
vscode.commands.registerCommand('nika.check', (uri) => { /* spawn nika check */ });
vscode.commands.registerCommand('nika.runTask', (uri, taskId) => { /* nika run --task */ });
vscode.commands.registerCommand('nika.dag', (uri) => { /* open DAG webview */ });
```

**Security**: `nika.run` command shows confirmation dialog before executing (exec: verb = arbitrary shell).

**Estimated LOC**: ~400 (LSP) + ~200 (VS Code extension)

---

### PR 6: Rename, References, Document Links, Folding

**Goal**: Complete the "standard LSP" feature set.

**4 new handlers**:

**rename.rs** (~300 LOC):
- Rename task ID → update ALL references:
  - `depends_on: [old_id]` → `depends_on: [new_id]`
  - `with: { alias: $old_id }` → `with: { alias: $new_id }`
  - Template refs in `prompt:` strings
- Cross-file rename for `include:` references
- Uses symbol_index for workspace-wide rename

**references.rs** (~200 LOC):
- Find all references to a task ID
- Sources: depends_on, with bindings, template expressions

**document_link.rs** (~150 LOC):
- `include: ./lib/common.nika.yaml` → Ctrl+Click opens file
- `fetch: { url: https://... }` → Ctrl+Click opens URL
- `pkg://supernovae/seo-tools` → Ctrl+Click opens registry

**folding.rs** (~150 LOC):
- Collapse each task block
- Collapse verb block
- Collapse multiline strings (`|`, `>`)
- Collapse `with:` block
- Collapse `mcp:` section

**Capabilities update** (capabilities.rs):
```rust
rename_provider: Some(OneOf::Right(RenameOptions {
    prepare_provider: Some(true),
    work_done_progress_options: Default::default(),
})),
references_provider: Some(OneOf::Left(true)),
document_link_provider: Some(DocumentLinkOptions { resolve_provider: Some(false) }),
folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
```

**Estimated LOC**: ~800 total

---

## Phase 3: Intelligence (PRs 7-8)

### PR 7: Elm-Style Error Messages + Full Error Coverage

**Goal**: Every NIKA error code surfaces in the LSP with actionable quick fixes.

**Current state**: 5 error codes handled (NIKA-140, 141, 142, 145, 160-164)
**Target**: 40+ error codes handled

**Error message philosophy** (stolen from Elm + Rust):
1. Name the error in human terms
2. Show exact code with pointers
3. Explain from the developer's perspective
4. Suggest concrete fixes
5. Link to documentation

**Example transformation**:

```
BEFORE:
  error[NIKA-050]: Unknown task 'reserach' in depends_on

AFTER:
  error[NIKA-050]: Unknown task reference

  --> workflow.nika.yaml:12:18
   |
12 |     depends_on: [reserach]
   |                  ^^^^^^^^ task 'reserach' not found
   |
   = did you mean 'research'?
   = available tasks: research, write_article, publish
   = help: https://nika.dev/docs/depends-on
```

**New code actions (quick fixes)** — expand from 4 to 20+:

| Error Code | Error | Quick Fix |
|------------|-------|-----------|
| NIKA-010 | Invalid schema version | Insert latest schema version |
| NIKA-011 | Unknown field | Remove field / suggest similar |
| NIKA-012 | Wrong type | Convert to correct type |
| NIKA-020 | DAG cycle | Remove one edge (suggest which) |
| NIKA-021 | Missing dependency | Add depends_on |
| NIKA-030 | Unknown provider | Suggest valid providers |
| NIKA-031 | Missing API key | Add env var instruction |
| NIKA-040 | Invalid template | Fix template syntax |
| NIKA-041 | Undefined binding | Add with: binding / suggest alias |
| NIKA-042 | Unknown transform | Suggest valid transforms |
| NIKA-050 | Unknown task ref | "Did you mean?" fuzzy match |
| NIKA-051 | Duplicate task ID | Rename / remove duplicate |
| NIKA-070 | Missing with: alias | Auto-generate with: block |
| NIKA-100 | MCP connection error | Check server config |
| NIKA-101 | Unknown MCP tool | Suggest available tools |
| NIKA-140 | Phase 2 analysis error | Context-specific fix |
| NIKA-160 | Parse syntax error | Insert missing `:`, fix indent |
| NIKA-161 | Missing required field | Insert field template |
| NIKA-300 | Structured output mismatch | Fix JSON schema |

**Estimated LOC**: ~1,200 (diagnostics expansion + code actions)

---

### PR 8: Live MCP Tool Discovery + Template Intelligence

**Goal**: Real completions from connected MCP servers + full `{{with.*}}` intelligence.

**mcp_tools.rs enhancements**:
1. Parse workflow's `mcp:` section for server configs
2. Cache MCP tool schemas (from previous runs or `nika mcp list`)
3. Provide completions for `invoke: { tool: <completion> }`
4. Provide parameter completions for each tool
5. Hover on tool name → show description, input schema, example

**template.rs enhancements**:
1. Complete `{{with.` → list all defined aliases in current task
2. Complete `{{inputs.` → list all workflow input parameters
3. Complete `{{context.` → list context file fields
4. Validate undefined references: `{{with.nonexistent}}` → warning
5. Transform completions: `{{with.data |` → `upper`, `lower`, `trim`, `to_yaml`, etc.
6. Transform validation: `{{with.data | nonexistent}}` → error

**signature_help.rs** (new):
- For `invoke:` blocks, show tool parameter signatures
- Trigger on `params:` → show required/optional params with types

**Estimated LOC**: ~800

---

## Phase 4: Magic (PRs 9-10)

### PR 9: DAG Visualization + Execution Trace

**Goal**: Visual workflow understanding directly in the editor.

**VS Code webview panel** showing:
- Interactive dependency graph (nodes = tasks, edges = depends_on)
- Color by verb: purple (infer), green (exec), blue (fetch), yellow (invoke), red (agent)
- Click node → jump to task definition
- Auto-updates on edit
- If trace file exists: overlay execution times and status (green/red)

**Implementation**:
- LSP custom request: `nika/dagGraph` → returns DAG as JSON
- VS Code extension: webview with D3.js or vis-network
- Optional: Mermaid diagram generation as fallback for non-VS Code editors

**Trace integration** (if `.nika-trace.ndjson` exists):
- Inlay hint: `[last: 2.3s, success]` after each task
- Code lens: `Last run: 2.3s | Success` or `Failed: timeout`
- Hover on task → show last output preview

**Estimated LOC**: ~600 (LSP) + ~800 (VS Code webview)

---

### PR 10: Formatting + Workspace Diagnostics + Workflow Linter

**Goal**: Production polish.

**formatting.rs** (~400 LOC):
- Opinionated Nika YAML formatting (like Prisma's formatter)
- Canonical key ordering: `schema` → `workflow` → `context` → `mcp` → `tasks`
- Task key ordering: `id` → `depends_on` → verb → `with` → `output` → `artifact`
- 2-space indent, consistent quoting
- Multiline string preservation
- Comment preservation

**Workspace diagnostics** (~300 LOC):
- Scan ALL `.nika.yaml` files on startup
- Report: broken includes, unused tasks, version conflicts
- LSP 3.17 diagnostic pull model (`textDocument/diagnostic`)

**Workflow linter** (~300 LOC):
- Beyond schema validation — best practices:
  - "Sequential tasks with no data dependency — consider parallelizing"
  - "No timeout set — will use default (300s)"
  - "Long prompt (>2000 chars) — consider extracting to file"
  - "Using deprecated schema version"
  - "Task has no description"
- Severity: Info/Hint (not Error/Warning)

**Estimated LOC**: ~1,000

---

## Error Code Coverage Map

### Currently Surfaced (5 codes)

| Code | Error | Diagnostic | Code Action |
|------|-------|-----------|-------------|
| NIKA-140 | Unknown task | YES | YES (fuzzy match) |
| NIKA-141 | Duplicate task | YES | YES |
| NIKA-142 | Invalid schema | YES | YES |
| NIKA-145 | Missing field | YES | YES |
| NIKA-160-164 | Parse errors | YES | NO |

### Target Coverage After Plan (40+ codes)

| Code Range | Category | Phase | Diagnostic | Code Action |
|------------|----------|-------|-----------|-------------|
| NIKA-000-009 | Workflow (parse fail, missing schema) | Phase 1 | YES | YES (insert schema) |
| NIKA-010-019 | Schema/validation (type, enum, field) | Phase 1 | YES | YES (fix type, suggest field) |
| NIKA-020-029 | DAG (cycles, missing deps, unreachable) | Phase 2 | YES | YES (remove edge, add dep) |
| NIKA-030-039 | Provider (unknown, key missing) | Phase 2 | YES | YES (suggest provider, env var) |
| NIKA-040-049 | Template/binding (syntax, undefined) | Phase 3 | YES | YES (fix syntax, add binding) |
| NIKA-050-059 | Task/path/security (unknown task, blocked) | Phase 1 | YES | YES (fuzzy match, unblock) |
| NIKA-060-069 | Output (JSON schema validation) | Phase 3 | YES | YES (fix schema) |
| NIKA-070-089 | With block + DAG validation | Phase 2 | YES | YES (add alias, fix ref) |
| NIKA-090-099 | JSONPath/IO | Phase 3 | YES | NO (runtime) |
| NIKA-100-109 | MCP (connection, tool, params) | Phase 3 | YES | YES (suggest tool) |
| NIKA-110-119 | Agent (tool invoke, loop) | Phase 3 | YES (warning) | NO |
| NIKA-120-129 | Resilience (retry, timeout) | Phase 3 | YES (info) | NO |
| NIKA-140-151 | AST analysis (Phase 2) | Phase 1 | YES | YES (expanded) |
| NIKA-160-164 | Parse errors (Phase 1) | Phase 1 | YES | YES (fix indent, add field) |
| NIKA-200-229 | File/builtin tools | — | NO (runtime) | — |
| NIKA-250-259 | Context errors | Phase 3 | YES | YES (fix path) |
| NIKA-260-269 | Package URI | Phase 4 | YES | YES (fix URI) |
| NIKA-280-289 | Artifacts | Phase 3 | YES (warning) | NO |
| NIKA-300-309 | Structured output | Phase 3 | YES | YES (fix schema) |

**Coverage: 5 codes → 40+ codes (8x expansion)**

---

## VS Code Extension Overhaul

### Current State (v0.1.0 — skeleton)

- Basic language activation on `*.nika.yaml`
- Launches `nika lsp` via `vscode-languageclient`
- Simple TextMate grammar (covers basics but missing many patterns)
- No custom commands, settings, webviews, or task providers
- No icons, no snippets, no configuration options

### Target State (v0.31.0 — production-grade)

**package.json enhancements**:

```json
{
  "contributes": {
    "configuration": {
      "title": "Nika",
      "properties": {
        "nika.server.path": { "type": "string", "default": "nika" },
        "nika.server.extraArgs": { "type": "array", "default": [] },
        "nika.trace.server": { "type": "string", "enum": ["off", "messages", "verbose"] },
        "nika.inlayHints.verb.enabled": { "type": "boolean", "default": true },
        "nika.inlayHints.dependencies.enabled": { "type": "boolean", "default": true },
        "nika.inlayHints.timeout.enabled": { "type": "boolean", "default": true },
        "nika.inlayHints.binding.enabled": { "type": "boolean", "default": true },
        "nika.codeLens.run.enabled": { "type": "boolean", "default": true },
        "nika.codeLens.validate.enabled": { "type": "boolean", "default": true },
        "nika.codeLens.dag.enabled": { "type": "boolean", "default": true },
        "nika.diagnostics.linter.enabled": { "type": "boolean", "default": true },
        "nika.formatting.enabled": { "type": "boolean", "default": true }
      }
    },
    "commands": [
      { "command": "nika.run", "title": "Nika: Run Workflow" },
      { "command": "nika.check", "title": "Nika: Validate Workflow" },
      { "command": "nika.runTask", "title": "Nika: Run Task" },
      { "command": "nika.dag", "title": "Nika: Show DAG" },
      { "command": "nika.newWorkflow", "title": "Nika: New Workflow" },
      { "command": "nika.restartServer", "title": "Nika: Restart Language Server" }
    ],
    "keybindings": [
      { "command": "nika.run", "key": "ctrl+shift+r", "when": "editorLangId == nika" },
      { "command": "nika.check", "key": "ctrl+shift+v", "when": "editorLangId == nika" }
    ],
    "snippets": [
      { "language": "nika", "path": "./snippets/nika.json" }
    ],
    "taskDefinitions": [
      { "type": "nika", "properties": { "workflow": { "type": "string" } } }
    ]
  }
}
```

**TextMate grammar improvements** (nika.tmLanguage.json):
- All 5 verbs with distinct scopes: `keyword.verb.infer.nika`, `keyword.verb.exec.nika`, etc.
- Template expressions: `{{with.alias}}` → `variable.other.template.nika`
- Binding references: `$task_id` → `variable.other.binding.nika`
- Schema version: `nika/workflow@0.12` → `string.other.schema.nika`
- Transforms in templates: `| upper` → `support.function.transform.nika`
- MCP server names in invoke blocks
- JSON Schema inside output.schema blocks

**Snippets** (snippets/nika.json):

```json
{
  "New Workflow": {
    "prefix": "workflow",
    "body": [
      "schema: nika/workflow@0.12",
      "workflow:",
      "  name: ${1:my-workflow}",
      "",
      "tasks:",
      "  - id: ${2:first_task}",
      "    infer:",
      "      model: ${3|claude-sonnet-4-5-20250514,claude-opus-4-0-20250514,gpt-4o|}",
      "      prompt: |",
      "        ${4:Your prompt here}",
      "$0"
    ]
  },
  "Infer Task": {
    "prefix": "infer",
    "body": [
      "  - id: ${1:task_name}",
      "    infer:",
      "      model: ${2|claude-sonnet-4-5-20250514,claude-opus-4-0-20250514|}",
      "      prompt: |",
      "        ${3:prompt}",
      "$0"
    ]
  },
  "Exec Task": {
    "prefix": "exec",
    "body": [
      "  - id: ${1:task_name}",
      "    exec:",
      "      command: ${2:echo hello}",
      "$0"
    ]
  },
  "Fetch Task": {
    "prefix": "fetch",
    "body": [
      "  - id: ${1:task_name}",
      "    fetch:",
      "      url: ${2:https://api.example.com}",
      "      method: ${3|GET,POST,PUT,DELETE|}",
      "$0"
    ]
  },
  "Invoke Task": {
    "prefix": "invoke",
    "body": [
      "  - id: ${1:task_name}",
      "    invoke:",
      "      server: ${2:novanet}",
      "      tool: ${3:novanet_search}",
      "      params:",
      "        ${4:query}: ${5:value}",
      "$0"
    ]
  },
  "Agent Task": {
    "prefix": "agent",
    "body": [
      "  - id: ${1:task_name}",
      "    agent:",
      "      prompt: |",
      "        ${2:goal}",
      "      model: ${3|claude-sonnet-4-5-20250514,claude-opus-4-0-20250514|}",
      "      max_turns: ${4:10}",
      "$0"
    ]
  },
  "With Binding": {
    "prefix": "with",
    "body": [
      "    with:",
      "      ${1:alias}: \\$${2:task_id}",
      "$0"
    ]
  }
}
```

---

## Performance Targets

| Metric | Current | Target | How |
|--------|---------|--------|-----|
| Completion latency | ~200ms (full reparse) | < 50ms | Cached snapshot, no reparse |
| Diagnostic latency | ~200ms (blocking) | < 200ms (non-blocking) | Debounced background analysis |
| Hover latency | ~100ms | < 30ms | Pre-computed PositionIndex |
| Position conversion | O(n) per call | O(log n) per call | LineIndex |
| Node lookup | O(n) linear scan | O(log n) binary search | PositionIndex |
| Memory (100 tasks) | ~10MB (3 text copies) | ~4MB (Arc<str>) | Single allocation |
| Memory (500 tasks) | ~50MB | ~15MB | Arc<str> + LineIndex |
| Startup | ~1s | < 1s | No change needed |
| Edit response (10 keystrokes) | 10 full reparses | 1 reparse (debounced) | AnalysisScheduler |
| Broken YAML completions | ZERO | Full completions | tree-sitter error recovery |

---

## Testing Strategy

### Test Pyramid

```
                    /\
                   /  \      Integration: 20 (real LSP protocol over stdio)
                  /----\
                 /      \    Snapshot: 100 (insta YAML snapshots per handler)
                /--------\
               /          \  Unit: 300+ (pure function tests per module)
              /____________\
```

### Test Categories

| Category | Count | Location | What |
|----------|-------|----------|------|
| Position/conversion | 80+ | `position.rs`, `protocol.rs` | UTF-16, emoji, CRLF, surrogate |
| WorldDatabase | 30+ | `db.rs` | Generation tracking, cache invalidation |
| Error recovery | 30+ | `parse/recovery.rs`, `bridge.rs` | Broken YAML fixtures |
| Context detection | 40+ | `analysis/context.rs` | All cursor contexts |
| Completion | 50+ | `handlers/completion.rs` | Per-context completions |
| Hover | 30+ | `handlers/hover.rs` | Verb/field/task hover docs |
| Definition | 20+ | `handlers/definition.rs` | Task refs, includes |
| Code action | 30+ | `handlers/code_action.rs` | Per-error-code fixes |
| Inlay hints | 20+ | `handlers/inlay_hints.rs` | All hint types |
| Rename | 15+ | `handlers/rename.rs` | Cross-reference updates |
| Snapshot | 100+ | `tests/snapshots/` | Full response snapshots |
| Integration | 20+ | `tests/integration/` | Real LSP protocol |

**Total: 350+ tests** (up from 179)

### Test Commands
```bash
# Safe unit tests (no keychain)
cargo test -p nika-lsp-core --lib

# With LSP integration tests
cargo test -p nika-lsp-core

# Snapshot updates
cargo insta test -p nika-lsp-core --review
```

---

## Architectural Decision Records

### ADR-1: Why not salsa?

**Context**: salsa provides incremental computation with automatic dependency tracking.

**Decision**: Use generation-based `WorldDatabase` instead.

**Rationale**:
- Our query graph is shallow (4 layers: text → parse → analyze → index)
- salsa adds significant compile-time cost and API complexity
- Generation-based approach gives 90% of benefit at 10% of complexity
- Can always migrate to salsa later if query graph grows

### ADR-2: Why tree-sitter-yaml for error recovery?

**Context**: No Rust YAML parser supports error recovery. marked_yaml fails completely on invalid YAML.

**Decision**: Use tree-sitter-yaml as a CST layer alongside marked_yaml.

**Rationale**:
- tree-sitter always produces a tree (with ERROR nodes for broken parts)
- It supports incremental re-parsing (edit + reparse in O(edit size))
- Already a dependency (TUI uses it for syntax highlighting)
- The bridge layer extracts PartialWorkflow from CST for basic LSP features
- marked_yaml remains for full-fidelity parsing when YAML is valid

**Trade-off**: Two parsers = two representations. Mitigated by clear layer separation (CST for recovery, marked_yaml for precision).

### ADR-3: Why consolidate into nika-lsp-core?

**Context**: Two implementations with 65% overlap, diverging features, double maintenance.

**Decision**: Single `nika-lsp-core` library crate used by both entry points.

**Rationale**:
- Eliminates all duplication
- Both entry points become ~50 lines of tower-lsp wiring
- Core is testable without any transport (pure functions)
- Can be embedded in nika binary, standalone binary, or even WASM

### ADR-4: Why not upgrade tower-lsp to 0.21?

**Context**: tower-lsp 0.21 supports newer LSP protocol features.

**Decision**: Stay on tower-lsp 0.20 + lsp-types 0.94 for Phase 1-2. Upgrade in Phase 3.

**Rationale**:
- tower-lsp 0.20 is stable and well-tested
- The upgrade can be done as a standalone PR without feature work
- lsp-types 0.94 covers all features through Phase 2 (inlay hints, code lens, rename)
- Phase 3 features (diagnostic pull model) benefit from 0.97

### ADR-5: Why opinionated formatting?

**Context**: YAML formatting can be configurable (Red Hat yaml-ls) or opinionated (Prisma).

**Decision**: Opinionated, one true format.

**Rationale**:
- Nika is a DSL, not generic YAML. We control the semantics.
- Reduces bikeshedding in teams
- Enables "format on save" without configuration debates
- Prisma proved this works for DSL ecosystems

---

## 10 Novel Nika-Only Features (Zero Prior Art)

These features exploit what makes Nika fundamentally different: it's a declarative DAG of AI tasks with data flow, cost implications, and runtime behavior that no other tool shares.

### Tier A: Foundation (build on existing AST/DAG)

| # | Feature | What | Effort | Impact |
|---|---------|------|--------|--------|
| 1 | **Data Flow Type Propagation** | Trace output schemas through DAG. `{{with.data.author}}` → ERROR if upstream schema has no `author` field. Completions for deep JSON paths from upstream schemas. | Medium | Catches deep property typos |
| 2 | **Template Scope Lens** | At any cursor in a prompt, show exactly what bindings are available: `with.*`, `inputs.*`, `context.*`, `platform.*` (if in for_each). Scope changes per task based on `with:` and `as:`. | Medium | No more "what can I access?" |
| 3 | **Transform Chain Validator** | Type-check pipe chains: `{{data \| sort \| first \| upper}}` — valid. `{{data \| upper \| sort}}` — ERROR: sort expects array, upper returns string. Context-aware completion after `\|`. | Low-Med | Only valid transforms suggested |
| 4 | **Binding Cycle & Deadlock Detector** | Detect implicit cycles through `with:` (not just `depends_on`). Cross-file deadlocks via `include:`. Unreachable task detection. Diamond dependency staleness warnings. | Low | Catches runtime deadlocks at edit time |

### Tier B: Intelligence (require additional computation)

| # | Feature | What | Effort | Impact |
|---|---------|------|--------|--------|
| 5 | **Prompt Quality Linter** | World's first in-editor prompt linter. Detects: `extended_thinking` on non-Claude model (error), redundant "Return JSON" when `output.format: json` (info), token budget exceeded (warning), injection risk from `{{with.user_input}}` (warning), missing format instruction. | Medium | Prompt engineering guidance in-editor |
| 6 | **Cost Radar** | Per-task cost inlay hints: `[~$0.03]`. Workflow total: `[est. $0.35-$1.20]`. Model hover: `$3/1M input, $15/1M output`. Provider comparison code action: "Switch to groq, save 93%". Uses existing `cost.rs` pricing tables. | Medium | Prevents cost surprises |
| 7 | **DAG Bottleneck Detection** | Critical path analysis at edit time. Mark bottleneck tasks: `[critical path: gates 4 tasks]`. Parallelism suggestions: "Tasks X and Y could run in parallel". Fan-out/fan-in imbalance warnings. Weight heuristics: exec=1s, fetch=2s, infer=5s, agent=5s*max_turns. | Medium | Optimize before running |

### Tier C: Magic (the "HOW did it know that?" features)

| # | Feature | What | Effort | Impact |
|---|---------|------|--------|--------|
| 8 | **MCP Tool Signature Overlay** | Live API docs from MCP servers. Complete `tool:` with actual server tools. Complete `params:` from tool input schemas. Validate required params. Hover shows description + example. Like IntelliSense for microservices. | High | Writing invoke: stops being guesswork |
| 9 | **Smart Scaffold** | Code action "Scaffold task" on empty `- id:`. Auto-generates `with:` from unbound upstream outputs, `depends_on:` from inferred deps, verb from task name ("validate" → infer, "save" → exec). DAG-aware code generation. | Med-High | "HOW did it know that?" |
| 10 | **Workflow Diff Preview** | Semantic DAG diffing vs last save/commit. Code lens: `[+1 task, -1 dependency, critical path unchanged]`. Gutter decorations for new/modified tasks. Model change cost impact: "switched opus → sonnet: 80% cost reduction". | High | Understand impact of edits |

---

## Revised Roadmap (Realistic, from brutal critique)

### v0.31 — "Already feels 11/10" (6 PRs, ~9 weeks)

```
PR 0 (Week 0): Upgrade tower-lsp to tower-lsp-server 0.23
  - Fix Cargo.toml comment (says v0.22, is v0.20)
  - Dedicated PR, no feature work

PR 1 (Weeks 1-2): nika-lsp-core + WorldDatabase + LineIndex
  - 12 files, ~1,500 LOC
  - 80+ tests (migrate position/conversion)
  - Property-based tests for position conversion (proptest)
  - Latency benchmark harness (criterion)
  - TDD: write handler test signatures FIRST, let them drive the API

PR 2 (Weeks 3-4): Error recovery (tree-sitter-yaml + bridge)
  - MANDATORY: bridge is STRUCTURE-ONLY (never trust ts-yaml value types)
  - 50+ broken YAML fixtures in fixtures/broken/
  - Insta snapshots for every PartialWorkflow extraction

PR 3a (Week 5): Handler migration — delegation layer
  - Wire nika-lsp-core handlers into both entry points
  - OLD code still exists alongside new delegation
  - Verify feature parity with both implementations

PR 3b (Week 6): Handler migration — delete old code
  - Only after PR 3a proven for 1+ week
  - Delete 19 files (10 embedded + 9 standalone)
  - 250+ tests in nika-lsp-core

PR 4 (Weeks 7-8): Inlay hints (3 types ON, rest OFF) + Code Lens
  - ON by default: dependency chain, timeout, binding source
  - Code Lens: Run Workflow, Validate (gated by Workspace Trust)
  - VS Code extension: commands, keybindings, status bar

PR 5 (Week 9): Rename + References + Folding + Document Links + Highlight
  - Rename task IDs across deps, bindings, templates
  - Document highlight (cursor on task ID lights up all references)
  - Call hierarchy (task dependency tree via standard LSP protocol)
```

### v0.32 — "Intelligence" (4 PRs, weeks 10-16)

```
PR 6: Elm-style errors + expanded error coverage (32% → 60%+)
PR 7: Template intelligence + transform chain validation + scope lens
PR 8: Prompt quality linter + cost radar inlay hints
PR 9: DAG bottleneck detection + smart scaffold code action
```

### v0.33 — "Magic" (3+ PRs, weeks 17+)

```
PR 10: Live MCP tool discovery (cached schemas)
PR 11: DAG visualization webview + execution trace overlay
PR 12: Formatting + workspace diagnostics + workflow linter
```

### What Was Cut from v0.31 (moved to v0.32/v0.33)

| Feature | Moved to | Why |
|---------|----------|-----|
| DAG webview | v0.33 | Requires VS Code webview (TypeScript, separate review) |
| Formatting | v0.33 | Opinionated formatter needs community input |
| Workspace diagnostics | v0.33 | Needs lazy loading design for 1000+ file workspaces |
| Signature help | v0.32 | Depends on MCP cache infrastructure |
| Cost estimates | v0.32 | Dynamic, needs accuracy validation |
| Prompt linter | v0.32 | Novel — needs user feedback before defaults |
| Workflow linter | v0.33 | Best practices need community validation |

---

## Testing Strategy (enhanced from critique)

### Broken YAML Fixture Corpus (50+ files)

```
fixtures/broken/
  missing-colon.nika.yaml          # tasks\n  research\n    infer
  incomplete-key.nika.yaml         # tasks:\n  - id: \n    infer:
  duplicate-verb.nika.yaml         # infer: ... and exec: ... in same task
  mixed-indentation.nika.yaml      # tabs + spaces
  truncated-file.nika.yaml         # file ends mid-value
  unicode-keys.nika.yaml           # emoji in task names
  empty-document.nika.yaml         # empty file
  comments-only.nika.yaml          # only # comments
  yaml-directive.nika.yaml         # %YAML 1.2
  multi-document.nika.yaml         # --- separators
  ... (40 more categorized by error type)
```

### Property-Based Tests (proptest)

```rust
proptest! {
    #[test]
    fn roundtrip_position_conversion(text in "[\\s\\S]{0,10000}") {
        let index = LineIndex::new(&text);
        for offset in 0..text.len() as u32 {
            let pos = index.offset_to_position(offset);
            let back = index.position_to_offset(pos);
            prop_assert_eq!(back, offset);
        }
    }

    #[test]
    fn cursor_context_is_total(yaml in valid_nika_yaml()) {
        // For any valid position, exactly one CursorContext variant is returned
        let db = db_with_doc(&yaml);
        for line in 0..yaml.lines().count() {
            let ctx = detect_context(&db, file, Position { line, character: 0 });
            prop_assert!(matches!(ctx, CursorContext::..));
        }
    }
}
```

### Latency Benchmarks (criterion)

```rust
fn bench_completion_100_tasks(c: &mut Criterion) {
    let db = db_with_large_workflow(100); // 100-task fixture
    c.bench_function("completion_100_tasks", |b| {
        b.iter(|| handle_completion(&db, file, middle_position))
    });
}
// Target: < 5ms for 100 tasks, < 50ms for 500 tasks
```

### Test Pyramid (final)

| Category | Count | What |
|----------|-------|------|
| Position/conversion + proptest | 100+ | UTF-16, emoji, CRLF, surrogate pairs |
| WorldDatabase | 30+ | Generation tracking, cross-file invalidation |
| Error recovery fixtures | 50+ | Broken YAML → PartialWorkflow snapshots |
| Context detection | 40+ | All cursor contexts, 3 strategies |
| Handler tests | 150+ | Per-context completions, hover, definition |
| Latency benchmarks | 10+ | criterion, warm/cold cache, rapid typing |
| Integration (LSP protocol) | 20+ | End-to-end request/response |
| **Total** | **400+** | Up from 179 |

---

## Summary: The Roadmap

```
v0.31 (9 weeks, 6 PRs): Already feels 11/10
  PR 0: tower-lsp upgrade
  PR 1: nika-lsp-core + WorldDatabase + benchmarks
  PR 2: Error recovery (50+ broken YAML fixtures)
  PR 3a/3b: Handler consolidation (delegation, then deletion)
  PR 4: Inlay hints + Code Lens + VS Code extension
  PR 5: Rename + References + Folding + Highlight + Call Hierarchy

v0.32 (weeks 10-16): Intelligence
  PR 6-9: Elm errors, template intelligence, prompt linter, cost radar,
          DAG bottleneck, smart scaffold

v0.33 (weeks 17+): Magic
  PR 10-12: Live MCP, DAG webview, formatting, workspace diagnostics
```

### The 7 Metrics That Prove 11/10

| Metric | 6.2/10 (today) | 11/10 (target) |
|--------|----------------|----------------|
| Error recovery | 0% (broken YAML = nothing) | 100% (everything works on broken code) |
| Error code coverage | 32% (30/95 codes) | 60%+ v0.31, 80%+ v0.32 |
| Feature completeness | 7/17 standard LSP | 14/17 v0.31 + 10 novel features v0.32 |
| Code duplication | 65% (two implementations) | 0% (single nika-lsp-core) |
| Completion latency | ~200ms (reparse) | < 50ms (cached snapshot) |
| Broken YAML fixtures | 0 | 50+ with insta snapshots |
| Novel features (zero prior art) | 0 | 10 (prompt linter, cost radar, DAG bottleneck...) |

**The result**: The first YAML workflow LSP that approaches rust-analyzer quality — AND has features no LSP has ever had, because no other tool is an AI workflow engine.
