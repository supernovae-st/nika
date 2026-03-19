# Research Report: World-Class LSP Architecture in Rust (2026)

**Date**: 2026-03-19
**Scope**: Building a production-grade LSP for the Nika YAML workflow engine
**Research depth**: 12 searches across 50+ sources, cross-referenced

---

## Executive Summary

The Rust LSP ecosystem in 2026 is mature but fragmented. Three viable transport crates
exist (`tower-lsp`, `async-lsp`, `lsp-server`), each with distinct trade-offs. The LSP
protocol itself is stable at **v3.17** (no 3.18 exists yet). The most impactful
architectural decisions for Nika's LSP are: (1) choosing the right incremental computation
strategy, (2) separating analysis from protocol, and (3) leveraging tree-sitter for
error-tolerant parsing of incomplete YAML documents.

Nika already has a `nika-lsp` crate with `tower-lsp 0.20`. This report recommends an
incremental modernization path rather than a rewrite.

---

## 1. LSP Transport Layer: tower-lsp vs async-lsp vs lsp-server

### Current State of the Ecosystem

| Crate | Design | State Model | Concurrency | Used By |
|-------|--------|-------------|-------------|---------|
| `tower-lsp` 0.20 | Async, Tower-based | `&self` (immutable) | Requires `Arc<Mutex>` | taplo, nika-lsp (current) |
| `tower-lsp-server` 0.23 | Community fork of tower-lsp | `&self` (immutable) | Same as tower-lsp | Community projects |
| `async-lsp` | Async, Tower layers | `&mut self` (mutable) | Native Tower middleware | Newer projects |
| `lsp-server` | Sync, event-loop | Lock-free, ordered | Single-threaded event loop | rust-analyzer |

### Key Architectural Decisions & WHY

**tower-lsp (current Nika choice)**:
- Pro: Familiar, well-documented, async-native.
- Con: The `&self` constraint forces all state mutations through `Arc<Mutex<T>>` or
  `DashMap`. This creates subtle data races when `didChange` notifications interleave
  with completion requests. Every handler must acquire locks, adding latency.
- Con: Cannot use custom Tower `Layer` implementations -- the `LspService` wrapper is
  opaque.

**async-lsp (recommended for greenfield)**:
- Pro: Supports `&mut self` -- state changes are sequential by default, eliminating an
  entire class of race conditions.
- Pro: Full Tower `Layer` composability -- add concurrency limits, panic recovery, tracing
  as middleware layers.
- Con: Smaller community, fewer examples.

**lsp-server (rust-analyzer's choice)**:
- Pro: Lock-free, synchronous event loop. Handlers run in order. Zero async overhead.
- Pro: Battle-tested at scale (rust-analyzer handles huge Rust monorepos).
- Con: Blocking model -- CPU-heavy analysis blocks the event loop unless you spawn threads
  manually. More boilerplate.

### Recommendation for Nika

**Stay on tower-lsp for now, plan migration to async-lsp.**

Rationale: Nika's `nika-lsp` already uses `tower-lsp 0.20` with `DashMap` and
`parking_lot`. A migration to `async-lsp` would give us `&mut self` (eliminating DashMap)
and middleware composability, but it's not urgent. The transport layer is the thinnest
part of an LSP -- the real work is in analysis.

Action items:
1. Update `tower-lsp` 0.20 -> `tower-lsp-server` 0.23 (community fork, actively maintained)
2. Update `lsp-types` 0.94 -> 0.97 (latest, matches tower-lsp-server)
3. Long-term: evaluate `async-lsp` when the `&self` locks become a bottleneck

Sources:
- https://github.com/tower-lsp-community/tower-lsp-server
- https://docs.rs/async-lsp/latest/async_lsp/
- https://github.com/ebkalderon/tower-lsp/issues/284
- https://lib.rs/crates/async-lsp

---

## 2. rust-analyzer Architecture Patterns

### How the Best LSP Works

rust-analyzer is organized as ~30 crates in a layered architecture:

```
Layer 4: rust-analyzer (binary)     -- LSP protocol, JSON-RPC
Layer 3: ide                        -- IDE features (completion, hover, goto-def)
Layer 2: hir (Higher IR)            -- Semantic model (types, name resolution)
Layer 1: syntax                     -- Parser + syntax tree (rowan green trees)
Layer 0: salsa database             -- Incremental computation engine
```

### Key Patterns Worth Adopting

**1. Demand-Driven Computation**
Nothing is computed upfront. When a hover request arrives, it triggers exactly the
computations needed (parse -> resolve names -> find type) and caches every intermediate
result. Next request reuses cached results if inputs haven't changed.

Why: Avoids wasting CPU on files the user isn't looking at.

**2. Cancellation**
Every analysis operation checks a cancellation token. When the user types another
character, in-flight requests are cancelled and restarted with the new document state.

Why: Without this, completion requests pile up and the LSP feels sluggish.

**3. Virtual File System (VFS)**
All file reads go through a VFS that tracks file versions. The VFS is the "input" layer
for Salsa -- changing a file bumps its version, which invalidates dependent computations.

Why: Decouples the analysis engine from the filesystem. Enables testing with in-memory
files and consistent snapshots.

**4. Snapshot Isolation**
The database supports concurrent read snapshots. The main thread handles LSP messages.
Worker threads get read-only snapshots for analysis. When a file changes, the main thread
creates a new revision -- workers on stale snapshots are cancelled.

Why: Zero-lock reads. No contention between "user is typing" and "analysis is running."

### Performance Numbers (2025-2026)

- Crate graph update after adding dependency: ~100ms (was seconds before Salsa migration)
- Parallel autocomplete: 2-5x faster after Salsa migration
- Memory: Can use gigabytes on large monorepos (target/ dirs)
- Startup: Loads workspace via `cargo metadata`, then incrementally indexes

Sources:
- https://rust-analyzer.github.io
- https://www.youtube.com/watch?v=tn6qwhMNBJo (RustNL 2025 Salsa talk)
- https://dasroot.net/posts/2026/02/rust-tooling-deep-dive-internals-performance-advanced-patterns/

---

## 3. tree-sitter-yaml for Error Recovery

### Why tree-sitter Matters for an LSP

Users edit broken YAML constantly. A strict YAML parser (serde, marked-yaml) simply fails
on incomplete documents. tree-sitter provides:

1. **Error-tolerant parsing**: Produces a partial AST even when the document is invalid
2. **Incremental reparsing**: Only reparses the changed region (~0.8ms per keystroke)
3. **Concrete syntax tree**: Preserves whitespace, comments, structure -- essential for
   formatting and precise diagnostics

### tree-sitter-yaml Grammar Quality (2026)

Repo: `tree-sitter-grammars/tree-sitter-yaml` (last updated 2025-05-28)

Handles well:
- Multi-document YAML (`---` separators)
- Flow/block styles
- Anchors/aliases
- Incomplete documents (partial keys, missing values)

Known limitations:
- Complex merge keys (`<<: *anchor`) can misparse
- Some vendor extensions need custom queries
- The grammar has only 14 GitHub stars -- small community

### Performance Benchmarks

```
Keystroke latency:
  tree-sitter parse:     0.8ms  (synchronous, main thread safe)
  LSP semantic analysis: ~200ms (async, debounced)

Memory for 1MB YAML file:
  tree-sitter AST:       ~2.1MB
  Tokens only:           ~187KB
```

### Best Practices for Nika

**Dual-layer architecture**:
```
Layer 1 (sync, <1ms):  tree-sitter parse -> structural tokens, folding, brackets
Layer 2 (async, debounced 200ms): Nika schema validation -> diagnostics, completions
```

This is what Nika's TUI already does (the `tui` feature depends on `tree-sitter` and
`tree-sitter-yaml`). The LSP should reuse this same dual-layer approach.

**Error node handling**:
```rust
// Walk the tree-sitter CST, collect ERROR nodes
fn collect_errors(node: tree_sitter::Node) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    if node.is_error() || node.is_missing() {
        errors.push(Diagnostic {
            range: ts_range_to_lsp(node.range()),
            severity: Some(DiagnosticSeverity::ERROR),
            message: format!("Syntax error: unexpected {}", node.kind()),
            ..Default::default()
        });
    }
    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            errors.extend(collect_errors(child));
        }
    }
    errors
}
```

**Incremental parse on didChange**:
```rust
fn on_did_change(&mut self, uri: &Url, changes: &[TextDocumentContentChangeEvent]) {
    let doc = self.documents.get_mut(uri);
    for change in changes {
        doc.rope.apply_edit(change);  // Update ropey rope
        let edit = tree_sitter::InputEdit { /* from change range */ };
        doc.tree.edit(&edit);         // Tell tree-sitter what changed
    }
    doc.tree = self.parser.parse(doc.rope.to_string(), Some(&doc.tree)).unwrap();
    // Tree-sitter only reparses the changed region
}
```

Sources:
- https://github.com/tree-sitter-grammars/tree-sitter-yaml
- https://lambdaland.org/posts/2026-01-21_tree-sitter_vs_lsp/
- https://github.com/neomutt/lsp-tree-sitter

---

## 4. Incremental Computation: Salsa vs Generation-Based Caching

### The Two Approaches

**Generation-based caching (simple)**:
- Global version counter. On any change, bump version.
- Cache entries tagged with version. Stale = recompute everything.
- Used by: taplo, simple LSPs, tree-sitter's built-in incremental mode.

**Salsa (fine-grained dependency graph)**:
- Every computation is a "query" with tracked dependencies.
- On change, only invalidate queries whose inputs actually changed.
- Used by: rust-analyzer, Lady Deirdre (similar concept).

### Trade-off Matrix

| Aspect | Salsa | Generation-Based |
|--------|-------|-----------------|
| **Precision** | Surgical: edit 1 step -> revalidate 1 step | Coarse: edit 1 step -> revalidate entire workflow |
| **Complexity** | High: macros, query traits, learning curve | Low: version stamp, HashMap |
| **Memory** | Higher baseline (dependency graph overhead) | Lower baseline but frequent full recomputes |
| **When it wins** | Deep dependency graphs, multi-file analysis | Simple/flat schemas, single-file analysis |
| **Setup time** | Days to weeks | Hours |
| **Concurrency** | Built-in snapshots, cancellation | Roll your own |

### Performance Numbers (from RustNL 2025 talk)

Salsa in rust-analyzer after migration:
- Crate graph updates: <100ms (was seconds)
- Parallel autocomplete: 2-5x faster
- Memory regression fixed: 5x improvement with better interning

### Recommendation for Nika

**Start with generation-based caching. Design the API so Salsa can be swapped in later.**

Rationale:
- Nika workflows are typically single-file, 50-500 lines. The dependency graph is shallow
  (workflow -> steps -> with bindings -> templates).
- Generation-based caching with ~200ms debounce will feel instant for these file sizes.
- Salsa's learning curve and macro overhead are not justified until multi-file analysis
  (e.g., `import:` directives, shared `with:` definitions) becomes a priority.

**Design for the future**:
```rust
/// Analysis trait -- abstract over caching strategy
trait WorkflowAnalyzer {
    fn parse(&self, uri: &Url) -> Arc<WorkflowAst>;
    fn validate(&self, uri: &Url) -> Arc<Vec<Diagnostic>>;
    fn completions(&self, uri: &Url, pos: Position) -> Vec<CompletionItem>;
}

/// V1: Generation-based
struct GenerationAnalyzer {
    generation: u64,
    cache: HashMap<Url, (u64, CachedAnalysis)>,
}

/// V2 (future): Salsa-based
// #[salsa::query_group(WorkflowStorage)]
// trait WorkflowDatabase { ... }
```

Sources:
- https://salsa-rs.github.io/salsa/about_salsa.html
- https://www.youtube.com/watch?v=tn6qwhMNBJo
- https://smallcultfollowing.com/babysteps/blog/2019/01/29/salsa-incremental-recompilation/

---

## 5. LSP Protocol: What to Support

### Current Protocol Version: 3.17 (latest as of March 2026)

There is **no LSP 3.18**. The protocol specification has been stable at 3.17 since its
release. The spec is at:
https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/

### Feature Priority Matrix for Nika

| Priority | Feature | LSP Method | Why for Nika |
|----------|---------|------------|--------------|
| **P0** | Diagnostics (push) | `textDocument/publishDiagnostics` | Schema validation errors, template errors |
| **P0** | Completion | `textDocument/completion` | Verb names, `with:` keys, provider names, model names |
| **P0** | Hover | `textDocument/hover` | Show step docs, template resolution, `with:` binding values |
| **P0** | Go to Definition | `textDocument/definition` | `with:` alias -> binding, template -> source |
| **P1** | Code Actions | `textDocument/codeAction` | Quick fixes: "did you mean `infer:`?", add missing `with:` binding |
| **P1** | Semantic Tokens | `textDocument/semanticTokens/full` | Highlight verbs, templates `{{}}`, keys differently |
| **P1** | Document Symbols | `textDocument/documentSymbol` | Outline view: workflow name, steps, with bindings |
| **P1** | Inlay Hints (3.17) | `textDocument/inlayHint` | Show resolved template values inline |
| **P2** | Folding Ranges | `textDocument/foldingRange` | Fold steps, `with:` blocks |
| **P2** | Document Links | `textDocument/documentLink` | Click to open referenced files |
| **P2** | Rename | `textDocument/rename` | Rename `with:` aliases across templates |
| **P2** | Diagnostics (pull) | `textDocument/diagnostic` | Better for workspace-wide diagnostics |
| **P3** | Workspace Symbols | `workspace/symbol` | Search across multiple .nika.yaml files |
| **P3** | Formatting | `textDocument/formatting` | YAML formatting (delegate to prettier/yamlfmt?) |

### Capabilities Declaration

```json
{
  "textDocumentSync": {
    "openClose": true,
    "change": 2,
    "save": { "includeText": false }
  },
  "completionProvider": {
    "triggerCharacters": [":", " ", ".", "{", "/"],
    "resolveProvider": true
  },
  "hoverProvider": true,
  "definitionProvider": true,
  "referencesProvider": true,
  "documentSymbolProvider": true,
  "codeActionProvider": {
    "codeActionKinds": ["quickfix", "source.fixAll"]
  },
  "semanticTokensProvider": {
    "full": true,
    "legend": {
      "tokenTypes": ["keyword", "variable", "string", "function", "property"],
      "tokenModifiers": ["declaration", "definition", "deprecated"]
    }
  },
  "inlayHintProvider": true,
  "foldingRangeProvider": true,
  "diagnosticProvider": {
    "interFileDependencies": false,
    "workspaceDiagnostics": false
  }
}
```

Sources:
- https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- https://buf.build/blog/protobuf-lsp

---

## 6. Crate Extraction Patterns

### How the Best Projects Structure Their Workspaces

**rust-analyzer** (~30 crates):
```
crates/
  syntax/          -- Parser, green trees (no deps on analysis)
  hir/             -- Semantic model
  ide/             -- IDE features
  proc-macro-srv/  -- Proc macro expansion
  rust-analyzer/   -- Binary, LSP protocol
```

**biome** (formerly Rome):
```
crates/
  biome_parser/       -- Language-agnostic parser infra
  biome_js_parser/    -- JS-specific parser
  biome_analyzer/     -- Lint rules, code actions
  biome_lsp/          -- LSP protocol (thin)
  biome_diagnostics/  -- Shared diagnostic types
  biome_cli/          -- CLI binary
```

**taplo** (TOML LSP):
```
crates/
  taplo-core/       -- TOML parser, AST, validation
  taplo-lsp/        -- LSP server (thin wrapper)
  taplo-formatter/  -- Formatting engine
  taplo-schema/     -- JSON Schema support
  taplo-cli/        -- CLI binary
```

### Common Pattern: The "Thin LSP" Principle

All three projects follow the same pattern:
1. **Core crate**: Pure analysis logic. No I/O, no protocol, no async. Takes `&str` or
   AST, returns results. This crate is the brain.
2. **LSP crate**: Thin adapter. Translates LSP messages into core crate calls.
   Handles protocol details (positions, URIs, JSON-RPC). This crate is the mouth.
3. **CLI crate**: Another thin adapter. Same core, different interface.

Why: The core crate can be tested without LSP infrastructure, reused from CLI, embedded
in TUI, compiled to WASM.

### Recommended Nika Workspace Layout

```
tools/
  nika/                  -- Binary (CLI + TUI)
    src/
      ast/               -- Already exists: AST types, parser
      lsp/               -- Current embedded LSP module
      ...
  nika-lsp/              -- Already exists: standalone LSP binary
    src/
      main.rs            -- LSP entry point (tower-lsp Server)
      backend.rs         -- Analysis backend
      completion.rs      -- Completion handler
      diagnostics.rs     -- Diagnostic handler
      ...
```

Current state: Nika has BOTH an embedded `src/lsp/` module (in the main binary) AND a
standalone `nika-lsp` crate. This duplication should be resolved.

**Recommended evolution**:

```
tools/
  nika/                  -- Binary (CLI + TUI), depends on nika-core
  nika-lsp/              -- LSP binary, depends on nika-core
  nika-core/             -- NEW: Pure analysis library (extracted from nika)
    src/
      lib.rs
      ast/               -- Workflow AST types + parser
      schema/            -- Schema validation (v0.12)
      completion/        -- Completion logic (verb names, with keys, etc.)
      diagnostics/       -- Diagnostic generation
      template/          -- Template parsing + resolution
      binding/           -- with: binding resolution
      hover/             -- Hover info generation
```

The extraction order:
1. Extract AST types (already well-defined in `nika/src/ast/`)
2. Extract schema validation logic
3. Extract template resolution
4. Build completion/hover/diagnostics on top
5. Both `nika` CLI and `nika-lsp` depend on `nika-core`

Sources:
- https://github.com/tamasfe/taplo
- https://gitnation.com/contents/whats-inside-biomes-linter
- https://rust-analyzer.github.io/manual.html

---

## 7. taplo Architecture: Lessons for a DSL-Focused LSP

### What taplo Gets Right

taplo is the closest analog to what Nika needs -- it's an LSP for a configuration DSL
(TOML), not a programming language. Key lessons:

**1. Schema-driven completion and validation**
taplo uses JSON Schema to validate TOML structure and drive completions. For Nika, the
workflow schema (@0.12) should drive the same:
- Complete verb names based on schema
- Validate step structure against schema
- Show schema descriptions on hover

**2. Multi-layer diagnostics**
```
Syntax errors  (tree-sitter / parser)     -> immediate, severity ERROR
Schema errors  (JSON Schema validation)   -> 200ms debounce, severity ERROR
Style warnings (formatting differences)   -> severity HINT
```

**3. Embeddable design**
taplo-core is usable without the LSP. The formatting engine, the validator, the parser --
all work standalone. This is exactly what Nika needs for `nika check` CLI command to
share logic with `nika-lsp`.

### What taplo Gets Wrong (Pitfalls to Avoid)

**1. No incremental parsing**
taplo does a full reparse on every change. For small TOML files this is fine. For Nika
workflows that could grow larger (especially with embedded prompts), this would be a
problem. Use tree-sitter incremental parsing from day one.

**2. Single maintainer collapse**
taplo's original maintainer stepped back in 2023-2024. The project forked into `oxc-toml`
and `tombi`. Lesson: Design the core crate API to be stable and well-documented so others
can contribute.

**3. Limited semantic tokens**
taplo provides basic syntax highlighting but doesn't leverage LSP semantic tokens. Nika
should invest in semantic tokens early -- highlighting `infer:`, `exec:`, `fetch:`,
`invoke:`, `agent:` verbs differently, and making `{{template}}` references visually
distinct.

Sources:
- https://taplo.tamasfe.dev
- https://github.com/tamasfe/taplo
- https://github.com/oxc-project/oxc-toml

---

## 8. Lady Deirdre: An Alternative Worth Watching

Lady Deirdre is a Rust framework that unifies lexing, parsing, semantic analysis, and LSP
serving into a single crate. It claims better incremental performance than tree-sitter.

### Assessment

| Aspect | Rating | Notes |
|--------|--------|-------|
| Performance | Promising | Author's benchmarks show it beating tree-sitter on ~2k LOC files |
| API Design | Good | Derive macros for grammar, task-based concurrency model |
| Maturity | Low | Solo author, non-FOSS license, limited adoption |
| Risk | High | Proprietary license, no ecosystem, unproven at scale |

### Recommendation

**Do not adopt for Nika.** The proprietary license and single-maintainer risk are
deal-breakers. However, the architectural patterns are worth studying:
- Task-based concurrency (MutationTask / AnalysisTask) is elegant
- Arena allocation for syntax trees is efficient
- The "Document" abstraction (text + tokens + tree + semantics) is a clean model

Sources:
- https://lady-deirdre.lakhin.com/semantics/language-server-design.html
- https://docs.rs/lady-deirdre

---

## 9. Performance Targets

### Latency Budgets for Nika LSP

| Operation | Target | Rationale |
|-----------|--------|-----------|
| `didChange` processing | <5ms | Must keep up with typing speed |
| Syntax diagnostics | <10ms | tree-sitter parse + error collection |
| Semantic diagnostics | <200ms | Schema validation, template resolution |
| Completion | <100ms p95 | User-perceived responsiveness threshold |
| Hover | <50ms p95 | Should feel instant |
| Go to Definition | <30ms | Single lookup in index |
| Document Symbols | <50ms | Walk AST once |
| Semantic Tokens | <100ms | Full file token classification |

### Memory Budget

| Metric | Target | Notes |
|--------|--------|-------|
| Baseline (idle) | <30MB | LSP server with no open files |
| Per open file | <2MB | tree-sitter AST + rope + cached analysis |
| 20 open files | <70MB | Typical workspace |

### Measurement Strategy

```rust
// Add tracing spans to every LSP handler
#[tracing::instrument(skip(self))]
async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let _timer = std::time::Instant::now();
    // ... handler logic ...
    tracing::info!(elapsed_ms = _timer.elapsed().as_millis(), "completion");
    Ok(result)
}
```

Sources:
- https://news.ycombinator.com/item?id=46719899
- https://www.tmdevlab.com/mcp-server-performance-benchmark.html

---

## 10. Common Pitfalls to Avoid

### 1. The DashMap Trap
Using `DashMap` for document storage seems convenient but causes subtle bugs:
- `didChange` and `completion` can interleave, reading half-updated state
- Fix: Use a single `RwLock<HashMap>` with proper write ordering, or migrate to
  `async-lsp` with `&mut self`

### 2. Blocking the Event Loop
Expensive operations (schema validation, template resolution) must not block LSP message
processing.
- Fix: Spawn analysis on a background task, debounce, cancel on new edits

### 3. Position Encoding Mismatch
LSP positions are UTF-16 offsets. Rust strings are UTF-8. Ropey uses byte offsets.
tree-sitter uses byte offsets. Every conversion is a potential off-by-one bug.
- Fix: Centralize position conversion in a single module. Test with emoji and CJK characters.

### 4. No Cancellation
Without cancellation, old requests pile up. User types 10 characters -> 10 completion
requests queue up -> LSP becomes unresponsive.
- Fix: Use `tokio_util::sync::CancellationToken`. Cancel in-flight analysis when document
  changes.

### 5. Monolithic Diagnostics
Sending all diagnostics (syntax + schema + template) in one batch means the user waits
for the slowest check.
- Fix: Send diagnostics incrementally. Syntax errors first (<10ms), then schema errors
  (~200ms), then template errors (~200ms).

### 6. Testing Without LSP Infrastructure
If analysis logic is embedded in LSP handlers, you can't test it without spinning up a
fake LSP client.
- Fix: Extract all analysis into `nika-core`. Test with plain `&str` -> `Vec<Diagnostic>`.

---

## 11. Actionable Recommendations for Nika

### Phase 1: Foundation (1-2 weeks)

1. **Resolve the LSP duplication**. Decide: `nika-lsp` standalone binary or `nika lsp`
   subcommand? Recommendation: Keep `nika-lsp` as standalone binary (easier for editor
   extensions to spawn), deprecate `src/lsp/` module.

2. **Update dependencies**:
   - `tower-lsp` 0.20 -> `tower-lsp-server` 0.23 (community fork)
   - `lsp-types` 0.94 -> 0.97
   - Verify `nika` dependency version alignment

3. **Add tree-sitter to nika-lsp**:
   - `tree-sitter` + `tree-sitter-yaml` as dependencies
   - Implement incremental parse on `didChange`
   - Use tree-sitter errors for instant syntax diagnostics

### Phase 2: Core Extraction (2-3 weeks)

4. **Create `nika-core` crate** (or expand nika-lsp's `backend.rs`):
   - Extract AST types from `nika/src/ast/`
   - Extract schema validation
   - Extract template parsing/resolution
   - All pure functions: `(&str, &Schema) -> Vec<Diagnostic>`

5. **Implement generation-based caching**:
   ```rust
   struct AnalysisCache {
       generation: u64,
       entries: HashMap<Url, CachedEntry>,
   }
   struct CachedEntry {
       generation: u64,
       tree: tree_sitter::Tree,
       ast: Arc<WorkflowAst>,
       diagnostics: Arc<Vec<Diagnostic>>,
   }
   ```

### Phase 3: Feature Completeness (3-4 weeks)

6. **P0 features**: Diagnostics, completion, hover, go-to-definition
7. **P1 features**: Code actions, semantic tokens, document symbols, inlay hints
8. **Cancellation**: Add `CancellationToken` to all analysis paths
9. **Incremental diagnostics**: Syntax first, then schema, then templates

### Phase 4: Polish (2-3 weeks)

10. **Performance benchmarks**: Instrument all handlers with tracing
11. **P2 features**: Folding, document links, rename
12. **VS Code extension**: Package nika-lsp as VS Code extension
13. **Testing**: Property-based tests for position conversion, fuzzing for parser

---

## 12. Recommended Code Patterns

### Pattern: Document Store with Versioned Analysis

```rust
use ropey::Rope;
use std::sync::Arc;

pub struct Document {
    pub uri: Url,
    pub version: i32,
    pub rope: Rope,
    pub tree: tree_sitter::Tree,
    // Lazily computed, cached until next edit
    pub analysis: Option<Arc<Analysis>>,
}

pub struct Analysis {
    pub generation: u64,
    pub syntax_diagnostics: Vec<Diagnostic>,   // From tree-sitter
    pub schema_diagnostics: Vec<Diagnostic>,   // From JSON Schema
    pub template_diagnostics: Vec<Diagnostic>, // From template resolution
    pub symbols: Vec<DocumentSymbol>,
    pub completions_cache: CompletionIndex,
}
```

### Pattern: Debounced Analysis with Cancellation

```rust
async fn schedule_analysis(uri: Url, cancel: CancellationToken) {
    // Debounce: wait 150ms for more edits
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(150)) => {}
        _ = cancel.cancelled() => return,
    }

    // Phase 1: Syntax (fast, from tree-sitter)
    let syntax_diags = analyze_syntax(&uri).await;
    publish_diagnostics(&uri, &syntax_diags).await;

    // Phase 2: Schema (slower)
    tokio::select! {
        diags = analyze_schema(&uri) => {
            let mut all = syntax_diags;
            all.extend(diags);
            publish_diagnostics(&uri, &all).await;
        }
        _ = cancel.cancelled() => return,
    }
}
```

### Pattern: Completion with Context

```rust
fn complete_at(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let node = find_tree_sitter_node_at(doc, pos);
    match classify_context(node) {
        Context::VerbPosition => complete_verbs(),         // infer:, exec:, fetch:, invoke:, agent:
        Context::WithKey => complete_with_bindings(doc),   // Available with: aliases
        Context::Template => complete_template_vars(doc),  // {{with.xxx}} variables
        Context::ProviderName => complete_providers(),     // openai, anthropic, mistral...
        Context::ModelName(provider) => complete_models(provider),
        Context::StepKey => complete_step_keys(),          // name:, with:, output:, ...
        _ => vec![],
    }
}
```

---

## Sources Summary

| # | Source | What It Provided |
|---|--------|-----------------|
| 1 | rust-analyzer.github.io | Architecture overview, layering patterns |
| 2 | RustNL 2025 Salsa talk | Salsa migration performance numbers |
| 3 | tower-lsp-community/tower-lsp-server | Community fork status, version info |
| 4 | docs.rs/async-lsp | async-lsp API, middleware pattern |
| 5 | ebkalderon/tower-lsp#284 | tower-lsp limitations discussion |
| 6 | tree-sitter-grammars/tree-sitter-yaml | Grammar quality, last update date |
| 7 | lambdaland.org tree-sitter vs LSP | Performance comparison, dual-layer pattern |
| 8 | salsa-rs.github.io | Salsa API, query model documentation |
| 9 | microsoft.github.io LSP 3.17 spec | Protocol features, capabilities |
| 10 | taplo.tamasfe.dev | taplo architecture, schema validation |
| 11 | biomejs.dev | Biome crate structure, multi-threading |
| 12 | lady-deirdre.lakhin.com | Alternative framework, task model |
| 13 | buf.build/blog/protobuf-lsp | Practical LSP implementation guide |
| 14 | dasroot.net Rust tooling deep dive | 2026 Rust tooling ecosystem status |

## Methodology

- **Tools used**: Perplexity AI (sonar model), 12 targeted searches
- **Pages analyzed**: 50+ sources cross-referenced
- **Time period covered**: 2024-2026, with emphasis on 2025-2026 developments
- **Cross-referencing**: Claims verified across multiple sources where possible

## Confidence Level

**High** for architectural patterns and crate choices (well-documented, multiple sources agree).
**Medium** for performance numbers (self-reported benchmarks, limited independent verification).
**Low** for "latest 2026 changes" (LSP protocol stable at 3.17, no major ecosystem shifts detected).

## Further Research Suggestions

- Benchmark tree-sitter-yaml vs marked-yaml for Nika's specific YAML subset
- Profile nika-lsp startup time and memory with 20+ open workflow files
- Evaluate `lsp-textdocument` crate for document sync (used by some projects instead of ropey)
- Study how yaml-language-server (Red Hat) handles schema-based completion for comparison
- Investigate `tower-lsp-server` 0.23 migration path from `tower-lsp` 0.20
