# Research: LSP Parsing Infrastructure for Nika

> **Date**: 2026-03-18
> **Context**: Nika LSP (nika-lsp) needs error-recovery parsing, incremental computation, and a proper CST for `.nika.yaml` files
> **Current stack**: `marked-yaml 0.8` (parser) + `tower-lsp 0.20` (LSP framework) + `tree-sitter-yaml 0.7` (TUI syntax highlighting)

---

## Table of Contents

1. [Error Recovery Parsing for YAML](#1-error-recovery-parsing-for-yaml)
2. [Rowan-style CST for YAML](#2-rowan-style-cst-for-yaml)
3. [Tree-sitter Integration in Rust LSPs](#3-tree-sitter-integration-in-rust-lsps)
4. [Salsa Framework for Incremental Computation](#4-salsa-framework-for-incremental-computation)
5. [Tower-lsp Best Practices](#5-tower-lsp-best-practices)
6. [Architecture Recommendations for Nika LSP](#6-architecture-recommendations-for-nika-lsp)

---

## 1. Error Recovery Parsing for YAML

### 1.1 tree-sitter-yaml

| Property | Value |
|----------|-------|
| **Crate** | `tree-sitter-yaml` |
| **Latest version** | **0.7.2** (Oct 2025) |
| **Grammar repo** | `tree-sitter-grammars/tree-sitter-yaml` (moved from `ikatyang/tree-sitter-yaml`) |
| **YAML spec** | 1.2 |
| **Error recovery** | Inherent via tree-sitter's GLR parser -- produces partial trees with `ERROR` nodes for malformed input |
| **Incremental** | Yes -- tree-sitter's core strength; O(n) on edits where n = change size |

**Strengths:**
- Built-in error recovery: tree-sitter always produces a tree, marking broken regions with `ERROR` nodes
- Incremental reparsing: only re-parses changed subtrees
- Already in Nika's dependency tree (used for TUI syntax highlighting)
- Language-agnostic query system for structural patterns

**Limitations:**
- Slow release cadence: fixes applied to master but not always tagged (build issues reported 2025)
- No semantic understanding -- purely syntactic
- CST is tree-sitter's internal format, not rowan-style; nodes are cursor-based, not persistent
- YAML's indentation sensitivity makes the grammar complex; potential edge cases with advanced YAML features (anchors, multi-line strings)
- No comment/whitespace preservation guarantee in the standard tree-sitter way (they become unnamed nodes)

**Verdict**: Best option for error-tolerant syntactic parsing. Pair with a semantic layer.

### 1.2 yaml-rust2

| Property | Value |
|----------|-------|
| **Crate** | `yaml-rust2` |
| **Latest version** | **0.11.0** |
| **Maintainer** | Ethiraric |
| **Status** | Maintenance mode -- stable API, no new features |
| **Successor** | `saphyr` (same author) for new features |

**Key facts:**
- Fork of unmaintained `yaml-rust` (RUSTSEC-2024-0320)
- Full YAML 1.2 compliance via test suite
- **No error recovery**: parser fails on invalid YAML
- **No CST or lossless parsing**: produces `Yaml` enum AST
- Pure Rust, no unsafe
- `marked-yaml` is built on top of yaml-rust2

**Verdict**: Not suitable for LSP error recovery. Good for final validation after tree-sitter provides structure.

### 1.3 saphyr

| Property | Value |
|----------|-------|
| **Crate** | `saphyr` 0.0.6 / `serde-saphyr` 0.0.21 |
| **Relationship** | Active successor to yaml-rust2 by same maintainer |
| **Status** | Early, less stable API, accepts new features |

- Same limitations as yaml-rust2 for error recovery
- Nika already uses `serde-saphyr` for deserialization
- No CST support

**Verdict**: Use for deserialization (already doing this), not for LSP parsing.

### 1.4 marked-yaml

| Property | Value |
|----------|-------|
| **Crate** | `marked-yaml` |
| **Latest version** | **0.8.0** |
| **Built on** | yaml-rust2 |
| **Key feature** | Span/Marker tracking (line:col for every node) |

**What it provides:**
- `Node` AST with `Span` (start/end positions) and `Marker` (offset, line, column)
- `Spanned<T>` wrapper for serde deserialization with location info
- Nika's current Phase 1 parser uses this for span-aware error reporting

**Limitations:**
- **No error recovery**: inherits yaml-rust2's fail-fast behavior
- **Not a CST**: loses comments, whitespace, formatting
- **No extensibility hooks** for custom recovery logic
- Constraints: top-level must be mapping/sequence, keys must be scalar strings, no anchors/aliases

**Verdict**: Good for Phase 1 raw parsing with spans. Cannot be extended for LSP error recovery.

### 1.5 Red Hat yaml-language-server

| Property | Value |
|----------|-------|
| **Technology** | TypeScript / Node.js |
| **Parser** | `eemeli/yaml` (npm `yaml` package) |
| **Repo** | `redhat-developer/yaml-language-server` |

**Architecture:**
- Uses `eemeli/yaml` which provides a **3-layer API**: Parse/Stringify, Documents, and **CST Parser**
- The CST layer preserves comments, whitespace, and enables round-trip editing
- Error recovery: `eemeli/yaml` produces partial documents on broken input, collecting errors separately
- Schema validation via JSON Schema (Draft 7) for structured diagnostics
- Detects: missing nodes, invalid key types, invalid types, extra properties

**Key insight**: The gold standard YAML LSP uses a parser with CST support. `eemeli/yaml`'s CST is what makes Red Hat's yaml-language-server work well with broken files.

### 1.6 Other YAML Parsers with CST/Lossless Parsing

| Parser | Language | CST? | Error Recovery? | Round-trip? |
|--------|----------|------|-----------------|-------------|
| `eemeli/yaml` | JS/TS | **Yes** | **Yes** (partial docs) | **Yes** |
| `ruamel.yaml` | Python | Partial (preserves comments/ordering) | Partial | **Yes** |
| `YAML::PP` | Perl | Limited | Limited | Partial |
| `libfyaml` | C | Event-based | Partial | No |
| **No Rust option** | -- | -- | -- | -- |

**Critical gap**: There is no Rust YAML parser with CST/lossless/error-recovery support. This is the fundamental problem.

---

## 2. Rowan-style CST for YAML

### 2.1 How Rowan Works

| Property | Value |
|----------|-------|
| **Crate** | `rowan` |
| **Latest version** | **0.16.1** |
| **Repo** | `rust-analyzer/rowan` |
| **Used by** | rust-analyzer, taplo, and others |

**Green/Red Tree Architecture:**

```
Green Tree (immutable, shared, bottom-up)
  GreenNode: { kind, children: [GreenNode | GreenToken] }
  GreenToken: { kind, text }
  - Structural sharing via Arc (identical subtrees reuse memory)
  - Stores relative offsets, not absolute
  - Thread-safe, cheap to clone

Red Tree (lazily computed, top-down)
  SyntaxNode: wraps GreenNode + parent pointer + absolute offset
  SyntaxToken: wraps GreenToken + context
  - On-the-fly construction during traversal
  - Provides parent links (green trees don't have these)
  - Enables typed API via Language trait
```

**Key properties:**
- **Lossless**: every byte of source is represented (whitespace, comments = tokens)
- **Cheap edits**: replace a subtree, share everything else
- **Dynamically typed**: `SyntaxKind` is a u16; typed wrappers provide safety
- **Language trait**: define node kinds, cast nodes to typed wrappers

### 2.2 cstree -- Alternative to Rowan

| Property | Value |
|----------|-------|
| **Crate** | `cstree` |
| **Latest version** | **0.13.0** |
| **Key difference** | Persistent red nodes (allocated once, stay alive) vs rowan's on-the-fly red nodes |

- Fork/reimagining of rowan with different memory trade-offs
- Once a red node is created, it stays allocated (rowan recreates on each traversal)
- Better for repeated traversals, worse for memory in large files
- Same green/red tree concept

### 2.3 Has Anyone Built a Rowan-based YAML Parser?

**No.** No public rowan-based YAML parser exists. The closest analog is **taplo** (TOML):

**Taplo architecture (the reference implementation):**
```
Source text
  --> Custom Lexer (tokenizer)
  --> Custom Parser (hand-written recursive descent)
  --> Rowan CST (lossless, preserves everything)
  --> DOM layer (semantic analysis, key resolution)
  --> LSP features (diagnostics, completion, formatting)
```

- Taplo uses rowan for its CST, NOT tree-sitter
- Parser collects errors separately from tree construction
- DOM works even with partial/broken trees
- This is exactly the pattern needed for Nika

### 2.4 What Would It Take to Build a Rowan YAML CST?

**Components needed:**

1. **SyntaxKind enum** (~40-60 variants for YAML DSL):
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
   #[repr(u16)]
   enum SyntaxKind {
     // Tokens
     WHITESPACE, NEWLINE, COMMENT, COLON, DASH, PIPE, GT,
     BARE_KEY, QUOTED_KEY, PLAIN_SCALAR, SINGLE_QUOTED, DOUBLE_QUOTED,
     BLOCK_SCALAR, INDENT, DEDENT, ANCHOR, ALIAS, TAG,
     // Composite nodes
     DOCUMENT, MAPPING, MAPPING_ENTRY, KEY, VALUE,
     SEQUENCE, SEQUENCE_ENTRY, FLOW_MAPPING, FLOW_SEQUENCE,
     // Nika-specific
     WORKFLOW_HEADER, TASK_BLOCK, VERB_BLOCK, WITH_BLOCK,
     TEMPLATE_EXPR, BINDING_REF,
     // Error
     ERROR,
     ROOT,
   }
   ```

2. **Lexer**: Indentation-aware tokenizer (YAML's biggest complexity)
   - Must track indent levels for INDENT/DEDENT tokens
   - Handle all YAML scalar styles (plain, quoted, block)
   - Preserve all whitespace and comments as tokens

3. **Parser**: Hand-written recursive descent (like taplo)
   - Error recovery: on unexpected token, emit ERROR node, skip to next reasonable point
   - Build GreenNode bottom-up via `GreenNodeBuilder`
   - Handle YAML's context-dependent grammar (block vs flow)

4. **Typed API**: Wrappers for Nika-specific nodes
   ```rust
   ast_node!(Workflow, DOCUMENT);
   ast_node!(Task, TASK_BLOCK);
   ast_node!(VerbBlock, VERB_BLOCK);
   ```

**Estimated effort**: 2-4 weeks for a basic YAML subset parser covering Nika's DSL needs. Full YAML 1.2 compliance would be 2-3 months.

**Alternative shortcut**: Use tree-sitter-yaml for parsing, convert to rowan green tree. This has been done for other languages but adds complexity.

---

## 3. Tree-sitter Integration in Rust LSPs

### 3.1 Typical Architecture Pattern

```
             tree-sitter                    Custom Semantic Layer
             (syntactic)                    (semantic)
                 |                               |
Source text --> Parse --> CST (tree-sitter) --> Convert --> Typed AST --> Analysis
                 |                                           |
         Error-tolerant                              Type checking
         Incremental                                 Binding resolution
         Fast (<1ms edits)                           Schema validation
```

### 3.2 How Other Rust LSPs Use Tree-sitter

| LSP | Tree-sitter Usage | Semantic Layer |
|-----|-------------------|----------------|
| **taplo** (TOML) | Does NOT use tree-sitter; uses rowan + custom parser | DOM layer on rowan CST |
| **rust-analyzer** | Does NOT use tree-sitter; uses rowan + custom parser | Salsa for incremental; HIR for semantics |
| **auto-lsp** | Generates typed AST FROM tree-sitter grammar | Salsa for caching; lsp_server for transport |
| **typst-lsp** | Tree-sitter for syntax queries | Custom engine for semantics |
| **SurrealQL LSP** | Tree-sitter for completions | Custom validation |

**Key observation**: The best LSPs (rust-analyzer, taplo) do NOT use tree-sitter. They use rowan + custom parsers for maximum control. Tree-sitter is used by simpler/lighter LSPs.

### 3.3 auto-lsp Framework

| Property | Value |
|----------|-------|
| **Crate** | `auto-lsp` |
| **Latest version** | **0.6.2** |
| **Repo** | `adclz/auto-lsp` |
| **Stars** | ~24 |
| **Maturity** | Early/experimental |

**What it does:**
- Auto-generates typed AST from tree-sitter's `node-types.json`
- Integrates tree-sitter + salsa + lsp_server
- Provides parent-child relations, downcasting, iteration
- WASM support

**Interesting but too immature for production use.**

### 3.4 Lady Deirdre

| Property | Value |
|----------|-------|
| **Crate** | `lady-deirdre` |
| **Latest version** | **2.2.0** |
| **Repo** | `Eliah-Lakhin/lady-deirdre` |
| **License** | Source-available (not fully open source) |

**What it does:**
- Complete framework for incremental parsers + LSP servers
- Own syntax tree representation (not rowan, not tree-sitter)
- Recursive-descent parser with unlimited lookahead
- Incremental reparsing built-in
- Claims better performance than tree-sitter in benchmarks
- No external dependencies beyond std

**Concerns:**
- Non-standard license (not MIT/Apache)
- Single-maintainer project
- Own ecosystem (not composable with rowan/salsa/tree-sitter)
- Small community

**Verdict**: Interesting research, not practical for Nika due to license and ecosystem isolation.

### 3.5 Performance Characteristics of tree-sitter for YAML

- **Initial parse**: ~1-5ms for typical YAML files (<1000 lines)
- **Incremental reparse**: <1ms for single-line edits
- **Memory**: ~10-50KB per parsed file for syntax tree
- **Error recovery**: Always produces a tree; ERROR nodes mark broken regions
- **Limitation**: YAML's indentation sensitivity makes the grammar complex; some edge cases may produce unexpected ERROR nodes

---

## 4. Salsa Framework for Incremental Computation

### 4.1 Current State

| Property | Value |
|----------|-------|
| **Crate** | `salsa` |
| **Latest version** | **0.26.0** (Feb 2026) |
| **Repo** | `salsa-rs/salsa` |
| **License** | Apache-2.0 OR MIT |
| **Used by** | rust-analyzer (via `rust-analyzer-salsa` fork), auto-lsp |

**Version history:**
- Old API: `#[salsa::query_group]` + `#[salsa::database]` (salsa 0.17 and earlier)
- New API: `#[salsa::tracked]` + `#[salsa::input]` + `#[salsa::db]` (salsa 0.18+, current 0.26)
- rust-analyzer uses its own fork (`rust-analyzer-salsa`) pinned to a specific version

### 4.2 New Salsa API (0.26)

```rust
// Define a salsa database
#[salsa::db]
pub trait Db: salsa::Database {}

// Input: data that comes from outside (file contents, settings)
#[salsa::input]
pub struct SourceFile {
    #[return_ref]
    pub text: String,
    pub uri: String,
}

// Tracked: derived data with automatic dependency tracking
#[salsa::tracked]
pub struct ParsedFile<'db> {
    #[return_ref]
    pub tree: SyntaxTree,
    #[return_ref]
    pub errors: Vec<ParseError>,
}

// Tracked function: cached, re-executed only when inputs change
#[salsa::tracked]
fn parse_file(db: &dyn Db, file: SourceFile) -> ParsedFile<'_> {
    let text = file.text(db);
    let (tree, errors) = parse_yaml(&text);
    ParsedFile::new(db, tree, errors)
}

#[salsa::tracked]
fn diagnostics(db: &dyn Db, file: SourceFile) -> Vec<Diagnostic> {
    let parsed = parse_file(db, file);
    // Only re-runs if parse_file's output changed
    validate(parsed.tree(db), parsed.errors(db))
}
```

### 4.3 Cancellation Mechanism

Salsa uses **stack unwinding** for cancellation:

```rust
// When input changes, any in-progress query is cancelled:
// 1. Main thread calls db.set_input(...)
// 2. This bumps the revision counter
// 3. Any query on a snapshot checks: is my revision stale?
// 4. If yes: Cancelled::throw() -- panics with a special payload
// 5. Caller catches the unwind with std::panic::catch_unwind

// Pattern for LSP:
fn handle_request(db: &RootDatabase, params: HoverParams) -> Option<Hover> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let file = resolve_file(db, &params.uri);
        let parsed = parse_file(db, file);
        compute_hover(db, parsed, params.position)
    }))
    .ok()
    .flatten()
}

// On didChange notification:
fn on_change(db: &mut RootDatabase, uri: &str, text: String) {
    // This automatically cancels any in-progress queries on snapshots
    let file = find_or_create_file(db, uri);
    file.set_text(db).to(text);
}
```

**Key insight**: Salsa's cancellation is synchronous. Snapshot-based queries on background threads get cancelled when the main thread mutates. This is why rust-analyzer uses a main thread + snapshot pattern, NOT async.

### 4.4 Usage Outside rust-analyzer

- **auto-lsp**: Uses salsa for parallel LSP request processing
- **Ruff** (Python linter): Uses salsa for incremental type checking (red-knot)
- Limited other adoption; most LSPs are too simple to need it

### 4.5 Performance

- Fine-grained invalidation: only re-computes what changed
- <10ms query latency for large codebases (rust-analyzer benchmarks)
- Memory: 1-10MB per database instance
- Overhead: 2-5x vs raw data structures due to tracking indirection
- **Overkill for single-file analysis** (Nika workflows are single files)

### 4.6 Do We Need Salsa for Nika LSP?

**Probably not yet.** Salsa shines for:
- Multi-file projects with complex dependency graphs
- Expensive analyses that benefit from caching across files
- Large codebases where full re-analysis is too slow

Nika workflows are:
- Typically single files (100-500 lines)
- Schema validation is fast (<5ms)
- No cross-file type system (yet)
- `include:` creates simple file dependencies

**Recommendation**: Start without salsa. Add it later if/when:
- Cross-file include resolution becomes expensive
- Package registry lookups need caching
- MCP schema validation is slow

---

## 5. Tower-lsp Best Practices

### 5.1 Current Landscape

| Crate | Version | Status | Architecture |
|-------|---------|--------|--------------|
| `tower-lsp` | **0.20.0** | **Stale** (original by ebkalderon) | Async, concurrent handlers |
| `tower-lsp-server` | **0.23.0** | **Active** (community fork) | Same, with fixes |
| `lsp-server` | **0.7.9** | **Active** (rust-analyzer) | Sync, crossbeam channels |
| `async-lsp` | **0.2.3** | **Active** | Async, proper Tower layers |

### 5.2 tower-lsp vs tower-lsp-server

The original `tower-lsp` (0.20.0) by ebkalderon is effectively archived. The community fork `tower-lsp-server` (0.23.0) continues maintenance with:
- Rust 2024 edition support
- Fixed workspace/symbol return type
- Cancellation panic fixes via `stream_select!`
- LSP 3.18 proposed features (behind feature flag)

**Nika currently depends on tower-lsp 0.20.0. Should migrate to tower-lsp-server 0.23.0.**

### 5.3 The Concurrency Problem

tower-lsp's fundamental design flaw (issue #284):
- All handlers get `&self` (immutable borrow)
- Requests and notifications run concurrently
- LSP spec requires notifications to be processed in order
- This can cause state drift (e.g., didChange processed after completion that references old state)

**Mitigations:**
1. Use `Arc<RwLock<State>>` with careful locking
2. Serialize notification processing manually
3. Or: switch to `lsp-server` (sync, single-threaded main loop) or `async-lsp` (mutable borrow for notifications)

### 5.4 Cancellation in tower-lsp

```rust
// tower-lsp does not provide built-in cancellation tokens.
// Pattern: use CancellationToken from tokio-util

use tokio_util::sync::CancellationToken;

struct Backend {
    client: Client,
    cancel_tokens: Arc<Mutex<HashMap<RequestId, CancellationToken>>>,
}

// On each request, create a token:
async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let token = CancellationToken::new();
    // Store token...
    tokio::select! {
        result = compute_hover(params) => result,
        _ = token.cancelled() => Ok(None),
    }
}

// On $/cancelRequest notification: cancel the token
```

### 5.5 Debouncing textDocument/didChange

tower-lsp dispatches didChange immediately. Manual debouncing:

```rust
use tokio::time::{Duration, Instant};

struct Backend {
    pending_changes: Arc<Mutex<HashMap<Url, (String, Instant)>>>,
}

async fn did_change(&self, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let text = full_text_from_params(&params);

    // Store latest change
    {
        let mut pending = self.pending_changes.lock().await;
        pending.insert(uri.clone(), (text, Instant::now()));
    }

    // Spawn debounced analysis
    let pending = self.pending_changes.clone();
    let client = self.client.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let should_analyze = {
            let pending = pending.lock().await;
            pending.get(&uri)
                .map(|(_, t)| t.elapsed() >= Duration::from_millis(300))
                .unwrap_or(false)
        };
        if should_analyze {
            let text = {
                let mut pending = pending.lock().await;
                pending.remove(&uri).map(|(t, _)| t)
            };
            if let Some(text) = text {
                let diagnostics = analyze(&text);
                client.publish_diagnostics(uri, diagnostics, None).await;
            }
        }
    });
}
```

### 5.6 Progress Reporting

```rust
use tower_lsp::lsp_types::*;

async fn long_operation(&self) {
    // Create progress token
    let token = NumberOrString::String("nika/analyze".into());
    self.client.send_request::<request::WorkDoneProgressCreate>(
        WorkDoneProgressCreateParams { token: token.clone() }
    ).await.ok();

    // Begin
    self.client.send_notification::<notification::Progress>(
        ProgressParams {
            token: token.clone(),
            value: ProgressParamsValue::WorkDone(
                WorkDoneProgress::Begin(WorkDoneProgressBegin {
                    title: "Analyzing workflow".into(),
                    cancellable: Some(true),
                    ..Default::default()
                })
            ),
        }
    ).await;

    // Report progress...
    // End...
}
```

### 5.7 Recommendation: Which LSP Framework?

| Option | Pros | Cons | For Nika? |
|--------|------|------|-----------|
| **tower-lsp-server 0.23** | Easy migration from 0.20; async; active | Concurrency footgun; `&self` everywhere | Short-term YES |
| **lsp-server 0.7.9** | Battle-tested (rust-analyzer); sync; no races | More boilerplate; no async handlers | Long-term BEST |
| **async-lsp 0.2.3** | Proper Tower layers; `&mut self` for notifications | Smaller community; less docs | Worth watching |

**Pragmatic path**: Migrate to `tower-lsp-server 0.23` now (minimal diff). Consider `lsp-server` if/when concurrency becomes a problem.

---

## 6. Architecture Recommendations for Nika LSP

### 6.1 Recommended Architecture

```
                    Nika LSP Architecture
                    =====================

  Editor (VS Code / Neovim / Zed)
      |
      | LSP Protocol (JSON-RPC over stdio)
      |
  tower-lsp-server 0.23 (transport layer)
      |
  NikaLanguageServer
      |
      +-- DocumentStore (HashMap<Url, Document>)
      |     |
      |     +-- Document
      |           |-- source_text: String (latest from editor)
      |           |-- tree: tree_sitter::Tree (incremental, error-tolerant)
      |           |-- version: i32
      |
      +-- Analyzer (semantic layer)
      |     |-- validate_structure(tree) -> Vec<Diagnostic>
      |     |-- resolve_bindings(tree) -> BindingMap
      |     |-- check_schema(tree) -> Vec<Diagnostic>
      |
      +-- Completer
      |     |-- complete_verb(position, tree) -> Vec<CompletionItem>
      |     |-- complete_binding(position, tree) -> Vec<CompletionItem>
      |     |-- complete_provider(position, tree) -> Vec<CompletionItem>
      |
      +-- HoverProvider
            |-- hover_verb(position, tree) -> Hover
            |-- hover_binding(position, tree) -> Hover
```

### 6.2 Parsing Strategy: Dual Parser

Use **both** tree-sitter-yaml and marked-yaml for complementary strengths:

```
Source Text
    |
    +-----> tree-sitter-yaml (always succeeds, error nodes)
    |           |-- Used for: completion, navigation, structure queries
    |           |-- Updated incrementally on each edit
    |           |-- Available immediately (< 1ms)
    |
    +-----> marked-yaml (strict, fails on errors)
                |-- Used for: full validation, schema checking
                |-- Run on debounced changes (300ms delay)
                |-- Provides span-accurate error messages
                |-- Feeds into existing Nika AST pipeline (Raw -> Analyzed -> Lower)
```

**Why dual parser?**
- tree-sitter gives instant feedback even on broken YAML
- marked-yaml gives precise, spec-compliant validation
- No need to build a custom rowan YAML parser (massive effort)
- Leverages existing Nika infrastructure

### 6.3 Phase Plan

**Phase 1 (Current)**: tower-lsp 0.20 + marked-yaml only
- Basic diagnostics on save
- Limited completion

**Phase 2 (Next)**: tower-lsp-server 0.23 + tree-sitter-yaml integration
- Migrate LSP framework
- Add tree-sitter parsing alongside marked-yaml
- Error-tolerant completion and navigation
- Debounced diagnostics

**Phase 3 (Future)**: Consider rowan CST or salsa if needed
- Only if cross-file analysis becomes expensive
- Only if formatting/refactoring needs lossless tree
- Consider when package registry / skill includes add complexity

### 6.4 Key Decision: NOT Building a Rowan YAML Parser (Yet)

Building a custom rowan-based YAML parser would be the "gold standard" approach (taplo did this for TOML). However:

| Factor | Custom Rowan Parser | Dual Parser (tree-sitter + marked-yaml) |
|--------|--------------------|-----------------------------------------|
| **Effort** | 2-4 weeks minimum | 2-3 days |
| **Error recovery** | Must implement manually | tree-sitter provides it for free |
| **Incremental** | Must implement manually | tree-sitter provides it for free |
| **Lossless** | Yes (preserves comments, whitespace) | No (but not needed yet) |
| **Formatting** | Enables nika fmt | Not possible without lossless tree |
| **Maintenance** | Must maintain YAML parser | Community maintains tree-sitter-yaml |

**Decision**: Use dual parser for now. Revisit rowan YAML parser when `nika fmt` becomes a priority.

---

## Appendix: Version Summary

| Crate | Latest | Nika Uses | Action |
|-------|--------|-----------|--------|
| `tree-sitter` | 0.26.7 | 0.24 (TUI) | Upgrade |
| `tree-sitter-yaml` | 0.7.2 | 0.7 (TUI) | Upgrade to 0.7.2 |
| `tower-lsp` | 0.20.0 | 0.20 (LSP) | **Migrate to tower-lsp-server 0.23** |
| `marked-yaml` | 0.8.0 | 0.8 | Keep |
| `rowan` | 0.16.1 | -- | Not needed yet |
| `cstree` | 0.13.0 | -- | Not needed yet |
| `salsa` | 0.26.0 | -- | Not needed yet |
| `lsp-server` | 0.7.9 | -- | Consider for future |
| `async-lsp` | 0.2.3 | -- | Watch |
| `lady-deirdre` | 2.2.0 | -- | Not recommended (license) |
| `auto-lsp` | 0.6.2 | -- | Too immature |
| `yaml-rust2` | 0.11.0 | via marked-yaml | Transitive |
| `saphyr` | 0.0.6 | via serde-saphyr | Keep |

## Sources

1. tree-sitter-grammars/tree-sitter-yaml -- GitHub (moved from ikatyang/tree-sitter-yaml)
2. Ethiraric/yaml-rust2 -- GitHub (issue #26: status)
3. redhat-developer/yaml-language-server -- GitHub
4. eemeli/yaml -- npm (CST parser documentation at eemeli.org/yaml/v1/)
5. rust-analyzer/rowan -- GitHub
6. tamasfe/taplo -- GitHub (architecture reference)
7. salsa-rs/salsa -- GitHub + crates.io
8. tower-lsp-community/tower-lsp-server -- GitHub
9. ebkalderon/tower-lsp -- GitHub (issue #284: concurrency)
10. rust-analyzer blog: "The Heart of a Language Server" (2023-12-26)
11. lambdaland.org: "Tree-sitter vs. LSP" (2026-01-21)
12. Eliah-Lakhin/lady-deirdre -- GitHub
13. adclz/auto-lsp -- GitHub
14. docs.rs/cstree, docs.rs/marked-yaml, docs.rs/tower-lsp
15. Perplexity AI searches conducted 2026-03-18

## Confidence Level

**High** for crate versions, architecture patterns, and feature comparisons.
**Medium** for performance numbers (benchmarks vary by hardware/workload).
**Low** for salsa internal API details (Perplexity results were inconsistent; some code examples may be outdated vs 0.26 API).
