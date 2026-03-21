# Nika LSP v3 — Implementation Plan

> **Date:** 2026-03-21
> **Baseline:** Gap analysis (same date), 273 core tests, 7,050 nika tests
> **Goal:** 4 tracks, 12 PRs, zero-duplication architecture, AI-specific novel features

---

## Architecture Overview

```
                    ┌─────────────────────────────┐
                    │       nika-lsp-core          │
                    │  (protocol-agnostic, pure)   │
                    │                              │
                    │  trait LspHandler             │
                    │  ├── completion()             │
                    │  ├── hover()                  │
                    │  ├── definition()             │
                    │  ├── code_action()            │
                    │  ├── semantic_tokens()        │
                    │  ├── symbols()                │
                    │  └── diagnostics()            │
                    │                              │
                    │  WorldDatabase (DashMap)      │
                    │  CursorContext (16 variants)  │
                    │  RecoveryParser (tree-sitter) │
                    └──────────┬───────────────────┘
                               │
                    ┌──────────┴───────────────┐
                    │                          │
         ┌──────────▼──────────┐   ┌──────────▼──────────┐
         │  nika (embedded)    │   │  nika-lsp (standalone)│
         │  src/lsp/server.rs  │   │  src/backend.rs       │
         │                     │   │                       │
         │  Thin tower-lsp     │   │  Thin tower-lsp       │
         │  shim: delegates    │   │  shim: delegates      │
         │  ALL handlers to    │   │  ALL handlers to      │
         │  nika-lsp-core      │   │  nika-lsp-core        │
         │                     │   │                       │
         │  + AST fallback     │   │  + MCP discovery      │
         │    (AstIndex)       │   │    fallback           │
         └─────────────────────┘   └───────────────────────┘
```

**After Track A completes:** Both binaries are ~100-200 LOC shims. All intelligence lives in
nika-lsp-core. ~10,670 LOC of duplicated handler code is deleted.

---

## Track A: Wire nika-lsp-core as Delegation Layer (3 PRs)

### PR-A1: Create Unified Handler Trait

**Goal:** Define `trait LspHandler` in nika-lsp-core so both binaries have a single dispatch point.

**Why a trait?** Today each handler is a free function (`hover(&str, u32, &CursorContext)`).
The trait bundles them under one interface, making it trivial for server.rs and backend.rs
to delegate: `self.handler.hover(params)`.

#### Task A1.1: Define `LspHandler` trait

**File:** `tools/nika-lsp-core/src/handler.rs` (NEW — ~120 LOC)

```rust
use crate::analysis::context::CursorContext;
use crate::handlers::{
    code_action::CodeActionEntry,
    definition::DefinitionResult,
    hover::HoverResult,
    semantic_tokens::RawToken,
    symbols::SymbolEntry,
};

/// Protocol-agnostic handler trait.
///
/// All methods take `(&self, text, offset/range, context)` and return
/// protocol-independent result types. The tower-lsp shim in each binary
/// converts these to `ls_types::*`.
pub trait LspHandler: Send + Sync {
    fn completion(&self, text: &str, offset: u32, context: &CursorContext)
        -> Vec<ls_types::CompletionItem>;

    fn hover(&self, text: &str, offset: u32, context: &CursorContext)
        -> Option<HoverResult>;

    fn definition(&self, text: &str, offset: u32, context: &CursorContext)
        -> Option<DefinitionResult>;

    fn code_action(&self, text: &str, start: u32, end: u32)
        -> Vec<CodeActionEntry>;

    fn semantic_tokens(&self, text: &str) -> Vec<RawToken>;

    fn symbols(&self, text: &str) -> Vec<SymbolEntry>;

    fn diagnostics(&self, text: &str) -> Vec<DiagnosticEntry>;
}
```

**Verification:**
- `cargo check -p nika-lsp-core` compiles
- No new dependencies added

#### Task A1.2: Implement `CoreHandler` struct

**File:** `tools/nika-lsp-core/src/handler.rs` (append — ~80 LOC)

```rust
/// Default implementation backed by pure-function handlers.
pub struct CoreHandler;

impl LspHandler for CoreHandler {
    fn completion(&self, text: &str, offset: u32, context: &CursorContext)
        -> Vec<ls_types::CompletionItem> {
        crate::handlers::completion::completions(text, offset, context)
    }

    fn hover(&self, text: &str, offset: u32, context: &CursorContext)
        -> Option<HoverResult> {
        crate::handlers::hover::hover(text, offset, context)
    }

    // ... each method delegates to the existing free function
}
```

**Verification:**
- `cargo test --lib -p nika-lsp-core` — all 273 tests pass
- New test: `CoreHandler` implements `Send + Sync` (compile-time check)

#### Task A1.3: Export trait from lib.rs

**File:** `tools/nika-lsp-core/src/lib.rs`

```diff
 pub mod analysis;
 pub mod db;
 pub mod document;
+pub mod handler;
 pub mod handlers;
 pub mod parse;
 pub mod position;
```

**Verification:**
- `cargo doc -p nika-lsp-core --no-deps` — trait appears in docs

#### Task A1.4: Define `DiagnosticEntry` result type

**File:** `tools/nika-lsp-core/src/handlers/diagnostics.rs` (NEW — ~60 LOC)

```rust
/// Protocol-agnostic diagnostic.
#[derive(Debug, Clone)]
pub struct DiagnosticEntry {
    pub offset: u32,
    pub end_offset: u32,
    pub severity: DiagnosticSeverity,
    pub code: String,       // "NIKA-042"
    pub message: String,
    pub source: &'static str, // "nika"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity { Error, Warning, Info, Hint }

/// Convert analysis text into diagnostics (thin wrapper for now).
pub fn diagnostics(_text: &str) -> Vec<DiagnosticEntry> {
    // Phase 1: empty — wired in PR-C
    vec![]
}
```

**File:** `tools/nika-lsp-core/src/handlers/mod.rs`

```diff
 pub mod code_action;
 pub mod completion;
 pub mod definition;
+pub mod diagnostics;
 pub mod hover;
 pub mod semantic_tokens;
 pub mod symbols;
```

**Verification:**
- `cargo test --lib -p nika-lsp-core` — all tests pass
- `DiagnosticEntry` is `Send + Sync`

---

**PR-A1 Summary:**

| Item | Value |
|------|-------|
| Files created | `handler.rs`, `handlers/diagnostics.rs` |
| Files modified | `lib.rs`, `handlers/mod.rs` |
| Estimated LOC | +260 |
| Tests required | 4 (trait object compiles, CoreHandler delegates, Send+Sync, diagnostics empty) |
| Dependencies | None (standalone PR) |
| Estimated time | 1 hour |

---

### PR-A2: Wire Delegation in Embedded LSP (nika/src/lsp)

**Goal:** Route ALL 6 handlers through nika-lsp-core first, with AST-aware fallback.
Today only `completion()` does this (server.rs:206-231). Extend the pattern to hover,
definition, symbols, code_action, and semantic_tokens.

**Depends on:** PR-A1

#### Task A2.1: Add CoreHandler to NikaLanguageServer

**File:** `tools/nika/src/lsp/server.rs` (lines 41-48)

```diff
 pub struct NikaLanguageServer {
     client: Client,
     documents: Arc<RwLock<DocumentStore>>,
     ast_index: AstIndex,
+    core: nika_lsp_core::handler::CoreHandler,
 }

 impl NikaLanguageServer {
     pub fn new(client: Client) -> Self {
         Self {
             client,
             documents: Arc::new(RwLock::new(DocumentStore::new())),
             ast_index: AstIndex::new(),
+            core: nika_lsp_core::handler::CoreHandler,
         }
     }
```

**Verification:**
- `cargo check -p nika --features lsp` compiles

#### Task A2.2: Wire hover delegation

**File:** `tools/nika/src/lsp/server.rs` (lines 233-247)

Replace the current direct `handlers::hover::compute_hover_with_ast` call with
core-first + fallback:

```rust
async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let docs = self.documents.read().await;
    let text = docs.get(uri).cloned().unwrap_or_default();

    // Try nika-lsp-core first
    let offset = super::conversion::position_to_offset(position, &text) as u32;
    let context = nika_lsp_core::analysis::context::detect_context(&text, offset, None);

    if let Some(result) = self.core.hover(&text, offset, &context) {
        return Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: result.contents,
            }),
            range: None,
        }));
    }

    // Fallback to AST-aware hover for model_intel, task context
    Ok(handlers::hover::compute_hover_with_ast(
        &self.ast_index, uri, &text, position,
    ))
}
```

**Verification:**
- `cargo test --lib -p nika --features lsp` — existing hover tests pass
- Manual: open a `.nika.yaml`, hover over `infer:` shows verb docs from core

#### Task A2.3: Wire definition delegation

**File:** `tools/nika/src/lsp/server.rs` (lines 249-266)

Same pattern: core-first, fallback to AST-aware.

```rust
async fn goto_definition(&self, params: GotoDefinitionParams)
    -> Result<Option<GotoDefinitionResponse>>
{
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let docs = self.documents.read().await;
    let text = docs.get(uri).cloned().unwrap_or_default();

    let offset = super::conversion::position_to_offset(position, &text) as u32;
    let context = nika_lsp_core::analysis::context::detect_context(&text, offset, None);

    if let Some(result) = self.core.definition(&text, offset, &context) {
        let range = super::conversion::offsets_to_range(
            result.offset, result.end_offset, &text,
        );
        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range,
        })));
    }

    // Fallback to AST-aware definition
    Ok(handlers::definition::find_definition_with_ast(
        &self.ast_index, uri, &text, position,
    ))
}
```

**Verification:**
- `cargo test --lib -p nika --features lsp` — definition tests pass
- Test: `depends_on: step1` navigates to `- id: step1`

#### Task A2.4: Wire code_action delegation

**File:** `tools/nika/src/lsp/server.rs` (lines 268-285)

```rust
async fn code_action(&self, params: CodeActionParams)
    -> Result<Option<CodeActionResponse>>
{
    let uri = &params.text_document.uri;
    let range = params.range;
    let diagnostics = &params.context.diagnostics;

    let docs = self.documents.read().await;
    let text = docs.get(uri).cloned().unwrap_or_default();

    let start = super::conversion::position_to_offset(range.start, &text) as u32;
    let end = super::conversion::position_to_offset(range.end, &text) as u32;

    // Core actions (schema, expand infer)
    let core_actions = self.core.code_action(&text, start, end);
    let mut actions: CodeActionResponse = core_actions
        .into_iter()
        .map(|a| convert_code_action_entry(a, uri))
        .collect();

    // AST-aware actions (fuzzy task match, diagnostic-based)
    let ast_actions = handlers::code_action::compute_code_actions_with_ast(
        &self.ast_index, uri, &text, range, diagnostics,
    );
    actions.extend(ast_actions);

    Ok(Some(actions))
}
```

Helper function `convert_code_action_entry` (~30 LOC) converts
`CodeActionEntry` (core) to `CodeActionOrCommand` (ls_types).

**Verification:**
- "Add schema version" action appears on files without `schema:`
- "Expand shorthand infer" action appears on `infer: "prompt"`

#### Task A2.5: Wire symbols and semantic_tokens delegation

**File:** `tools/nika/src/lsp/server.rs` (lines 287-341)

```rust
async fn document_symbol(&self, params: DocumentSymbolParams)
    -> Result<Option<DocumentSymbolResponse>>
{
    let uri = &params.text_document.uri;
    let docs = self.documents.read().await;
    let text = docs.get(uri).cloned().unwrap_or_default();

    // Try core symbols first
    let core_symbols = self.core.symbols(&text);
    if !core_symbols.is_empty() {
        let lsp_symbols = core_symbols.into_iter()
            .map(|s| convert_symbol_entry(s, &text))
            .collect();
        return Ok(Some(DocumentSymbolResponse::Nested(lsp_symbols)));
    }

    // Fallback to AST-aware
    let symbols = handlers::symbols::compute_document_symbols_with_ast(
        &self.ast_index, uri, &text,
    );
    Ok(Some(DocumentSymbolResponse::Nested(symbols)))
}

async fn semantic_tokens_full(&self, params: SemanticTokensParams)
    -> Result<Option<SemanticTokensResult>>
{
    let uri = &params.text_document.uri;
    let docs = self.documents.read().await;
    let text = docs.get(uri).cloned().unwrap_or_default();

    // Try core semantic tokens first
    let core_tokens = self.core.semantic_tokens(&text);
    if !core_tokens.is_empty() {
        let encoded = nika_lsp_core::handlers::semantic_tokens::encode_tokens(&core_tokens);
        return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: encoded,
        })));
    }

    // Fallback to existing
    let raw = handlers::semantic_tokens::compute_semantic_tokens_with_ast(
        &self.ast_index, uri, &text,
    );
    let encoded = handlers::semantic_tokens::encode_tokens(raw);
    Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: encoded,
    })))
}
```

**Verification:**
- `cargo test --lib -p nika --features lsp` — all tests pass
- Outline view shows tasks hierarchy
- Semantic tokens color verbs, keywords, templates

#### Task A2.6: Add conversion helpers

**File:** `tools/nika/src/lsp/conversion.rs` (append ~80 LOC)

```rust
/// Convert byte offsets to LSP Range.
pub fn offsets_to_range(start: u32, end: u32, text: &str) -> Range {
    Range {
        start: offset_to_position(start as usize, text),
        end: offset_to_position(end as usize, text),
    }
}

/// Convert core SymbolEntry to LSP DocumentSymbol.
pub fn convert_symbol_entry(entry: SymbolEntry, text: &str) -> DocumentSymbol { ... }

/// Convert core CodeActionEntry to LSP CodeActionOrCommand.
pub fn convert_code_action_entry(entry: CodeActionEntry, uri: &Uri)
    -> CodeActionOrCommand { ... }
```

**Verification:**
- Unit tests for `offsets_to_range` with multi-line text
- Round-trip: `position_to_offset(offset_to_position(n))` == n

---

**PR-A2 Summary:**

| Item | Value |
|------|-------|
| Files modified | `server.rs`, `conversion.rs` |
| Estimated LOC | +200 (net, replacing existing code) |
| Tests required | 6 (one per handler + conversion helpers) |
| Dependencies | PR-A1 |
| Estimated time | 2 hours |

---

### PR-A3: Wire Delegation in Standalone LSP (nika-lsp)

**Goal:** Apply the same core-first delegation pattern to `nika-lsp/src/backend.rs`.
This makes nika-lsp feature-identical to embedded for the 6 core handlers.

**Depends on:** PR-A1

#### Task A3.1: Add nika-lsp-core dependency

**File:** `tools/nika-lsp/Cargo.toml`

```diff
 [dependencies]
+nika-lsp-core = { path = "../nika-lsp-core" }
 tower-lsp-server = "0.23"
 # ... existing deps
```

**Verification:**
- `cargo check -p nika-lsp` compiles

#### Task A3.2: Add CoreHandler to NikaBackend

**File:** `tools/nika-lsp/src/backend.rs` (line 33-40)

```diff
 pub struct NikaBackend {
     client: Client,
     documents: DashMap<Uri, DocumentState>,
     validation_tx: mpsc::Sender<ValidationRequest>,
+    core: nika_lsp_core::handler::CoreHandler,
 }
```

Update `new()` to initialize it.

**Verification:**
- `cargo check -p nika-lsp` compiles

#### Task A3.3: Wire completion (already partially done — clean up)

**File:** `tools/nika-lsp/src/backend.rs` (lines 238-285)

The standalone already delegates to nika-lsp-core for completion. Clean up:
remove the dead `get_completion_context` import, simplify the fallback to
MCP-discovery only.

**Verification:**
- `cargo test -p nika-lsp` — completion tests pass

#### Task A3.4: Wire hover

**File:** `tools/nika-lsp/src/backend.rs` (lines 294-304)

Replace `get_hover(&doc, position)` with core-first:

```rust
async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let text = match self.documents.get(uri) {
        Some(d) => d.content(),
        None => return Ok(None),
    };

    let offset = position_to_offset(&text, position);
    let context = nika_lsp_core::analysis::context::detect_context(&text, offset, None);

    if let Some(result) = self.core.hover(&text, offset, &context) {
        return Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: result.contents,
            }),
            range: None,
        }));
    }

    // Fallback to legacy hover
    let doc = self.documents.get(uri).unwrap();
    Ok(crate::hover::get_hover(&doc, position))
}
```

**Verification:**
- Hover on `infer:` in VS Code with standalone server shows verb docs

#### Task A3.5: Wire definition

**File:** `tools/nika-lsp/src/backend.rs` (lines 308-359)

Same pattern: core-first, fallback to `ast_integration::find_task_by_id`.

**Verification:**
- Go-to-definition on `depends_on: step1` navigates correctly

#### Task A3.6: Add symbols, semantic_tokens, code_action capabilities

**File:** `tools/nika-lsp/src/backend.rs`

The standalone currently advertises only completion, hover, definition, and diagnostics.
Add capabilities for the 3 missing features and implement them by delegating to core:

```rust
// In initialize():
document_symbol_provider: Some(OneOf::Left(true)),
semantic_tokens_provider: Some(/* ... */),
code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
```

Implement `document_symbol()`, `semantic_tokens_full()`, `code_action()` using
`self.core.*` — no legacy fallback needed (standalone never had these).

**Verification:**
- Outline view works in VS Code with standalone server
- Semantic tokens color verbs
- "Add schema" code action appears

---

**PR-A3 Summary:**

| Item | Value |
|------|-------|
| Files modified | `Cargo.toml`, `backend.rs` |
| Estimated LOC | +150 (new), -100 (removed legacy hover/def) = +50 net |
| Tests required | 5 (one per new handler + capabilities check) |
| Dependencies | PR-A1 (PR-A2 is independent, can be parallel) |
| Estimated time | 2 hours |

---

## Track B: Add v0.35.x Field Completions (2 PRs)

### PR-B1: Fetch Extract Completions

**Goal:** Add completions for `extract:`, `selector:`, and `response:` fields in the
`fetch:` verb block. These were added in v0.35.0 (PR5) but the LSP has no knowledge of them.

**Depends on:** None (pure nika-lsp-core, no wiring needed)

#### Task B1.1: Add extract modes to fetch verb block

**File:** `tools/nika-lsp-core/src/handlers/completion.rs` (line 298-304)

```diff
 "fetch" => vec![
     item_snippet_fmt("url", ...),
     item_snippet_fmt("method", ...),
     item_snippet_fmt("headers", ...),
     item_snippet_fmt("body", ...),
     item_snippet_fmt("retry", ...),
+    item_snippet_fmt(
+        "extract",
+        CompletionItemKind::PROPERTY,
+        "extract: ${1|markdown,article,text,selector,metadata,links,jsonpath,feed,llm_txt|}",
+        "Post-processing mode for response body. 9 modes available.",
+        "5_extract",
+    ),
+    item_snippet_fmt(
+        "selector",
+        CompletionItemKind::PROPERTY,
+        "selector: ${1:CSS selector or JSONPath}",
+        "CSS selector (with extract: text/selector) or JSONPath (with extract: jsonpath).",
+        "6_selector",
+    ),
+    item_snippet_fmt(
+        "response",
+        CompletionItemKind::PROPERTY,
+        "response: ${1|full,binary|}",
+        "Response mode: full (JSON with headers), binary (CAS hash).",
+        "7_response",
+    ),
 ],
```

**Verification:**
- Test: `verb_block_completions("fetch", "")` returns 8 items (was 5)
- Test: `verb_block_completions("fetch", "ex")` returns item with label "extract"
- Test: `verb_block_completions("fetch", "sel")` returns "selector"

#### Task B1.2: Add extract-specific hover docs

**File:** `tools/nika-lsp-core/src/handlers/hover.rs` (inside `field_hover`)

```diff
     "timeout" => "**Timeout** — Max seconds.",
+    "extract" => "**Extract** — Post-process fetch response.\n\n| Mode | Description |\n|------|-------------|\n| `markdown` | Clean Markdown via htmd |\n| `article` | Main article (Readability) |\n| `text` | Visible text, optionally filtered by `selector:` |\n| `selector` | Raw HTML of matching elements |\n| `metadata` | OG/Twitter/JSON-LD/SEO tags |\n| `links` | Classified link list |\n| `jsonpath` | JSONPath on JSON responses |\n| `feed` | RSS/Atom/JSON Feed |\n| `llm_txt` | AI content discovery |",
+    "selector" => "**Selector** — CSS selector (for `extract: text/selector`) or JSONPath expression (for `extract: jsonpath`).",
+    "response" => "**Response Mode** — `full` returns JSON with status/headers/body. `binary` stores in CAS, returns hash.",
     _ => return None,
```

**Verification:**
- Test: `field_hover("extract")` returns Some with 9 modes listed
- Test: `field_hover("response")` returns Some

#### Task B1.3: Add CursorContext detection for extract values

**File:** `tools/nika-lsp-core/src/analysis/context.rs`

The `VerbBlock` context already captures `existing_subfields`. When the cursor is on the
VALUE side of `extract:`, we need a new sub-context. Add a match in the verb-block
detection logic:

```rust
// Inside detect_verb_block_context():
if key == "extract" && cursor_on_value {
    // Return VerbBlock with prefix = value prefix, so completions filter
    // against extract modes
}
```

This is a refinement, not a new variant. The existing `VerbBlock { verb: "fetch", prefix }`
path handles it when `prefix` matches against extract mode names.

**Verification:**
- Test: cursor after `extract: mar` with prefix "mar" -> filters to "markdown"

---

**PR-B1 Summary:**

| Item | Value |
|------|-------|
| Files modified | `completion.rs`, `hover.rs`, `context.rs` |
| Estimated LOC | +80 |
| Tests required | 6 |
| Dependencies | None |
| Estimated time | 1 hour |

---

### PR-B2: Agent + Infer New Field Completions

**Goal:** Add completions for fields added in v0.34-v0.35 that are missing from the LSP.

**Depends on:** None

#### Task B2.1: Agent new fields

**File:** `tools/nika-lsp-core/src/handlers/completion.rs` (inside `"agent"` match arm, line 311-321)

Add these missing fields after the existing agent completions:

```rust
item_snippet_fmt(
    "tool_choice",
    CompletionItemKind::PROPERTY,
    "tool_choice: ${1|auto,required,none|}",
    "How the agent selects tools. Default: auto.",
    "5_tool_choice",
),
item_snippet_fmt(
    "stop_sequences",
    CompletionItemKind::PROPERTY,
    "stop_sequences: [${1}]",
    "Custom stop sequences for generation.",
    "5_stop_sequences",
),
item_snippet_fmt(
    "scope",
    CompletionItemKind::PROPERTY,
    "scope: ${1|read,write,execute|}",
    "Permission scope for agent tools.",
    "6_scope",
),
```

**Verification:**
- Test: `verb_block_completions("agent", "")` returns 12 items (was 9)
- Test: `verb_block_completions("agent", "tool_c")` returns "tool_choice"

#### Task B2.2: Infer new fields

**File:** `tools/nika-lsp-core/src/handlers/completion.rs` (inside `"infer"` match arm, line 285-293)

```rust
item_snippet_fmt(
    "response_format",
    CompletionItemKind::PROPERTY,
    "response_format: ${1|json,text|}",
    "Force response format. Use with structured: for JSON schema.",
    "5_response_format",
),
item_snippet_fmt(
    "stop_sequences",
    CompletionItemKind::PROPERTY,
    "stop_sequences: [${1}]",
    "Custom stop sequences.",
    "5_stop_sequences",
),
item_snippet_fmt(
    "extended_thinking",
    CompletionItemKind::PROPERTY,
    "extended_thinking: true\nthinking_budget: ${1:8192}",
    "Enable extended thinking (Claude only).",
    "6_extended_thinking",
),
```

**Verification:**
- Test: `verb_block_completions("infer", "")` returns 10 items (was 7)
- Test: `verb_block_completions("infer", "res")` returns "response_format"

#### Task B2.3: Output block completions (max_retries)

**File:** `tools/nika-lsp-core/src/handlers/completion.rs` (inside `schema_block_completions`)

```rust
// Add to SchemaBlock completions:
item_snippet_fmt(
    "max_retries",
    CompletionItemKind::PROPERTY,
    "max_retries: ${1:3}",
    "Max retries for structured output validation.",
    "3_max_retries",
),
```

**Verification:**
- Test: `schema_block_completions("")` includes "max_retries"

#### Task B2.4: Add hover docs for new fields

**File:** `tools/nika-lsp-core/src/handlers/hover.rs` (inside `field_hover`)

Add entries for: `tool_choice`, `stop_sequences`, `scope`, `response_format`,
`extended_thinking`, `max_retries`.

**Verification:**
- Test: each new field returns Some from `field_hover`

---

**PR-B2 Summary:**

| Item | Value |
|------|-------|
| Files modified | `completion.rs`, `hover.rs` |
| Estimated LOC | +120 |
| Tests required | 8 |
| Dependencies | None |
| Estimated time | 1 hour |

---

## Track C: Port Handlers to nika-lsp-core (4 PRs)

These PRs make the nika-lsp-core handlers production-quality by porting the rich
logic from the embedded LSP's 1,000+ LOC handlers. After each port, the corresponding
core handler should produce results equivalent to the embedded handler for all test cases.

### PR-C1: Port Hover (86 LOC -> ~600 LOC)

**Goal:** The current core hover (86 LOC) covers verbs, basic fields, and root keys.
The embedded hover (1,008 LOC) also covers: provider hover with pricing, model hover
with capabilities, template variable hover showing bound task output, task ID hover
with dependency info, transform documentation, content/vision part docs.

**Depends on:** None (core-only, no wiring changes)

#### Task C1.1: Add provider hover

**File:** `tools/nika-lsp-core/src/handlers/hover.rs` (append ~80 LOC)

Handle `CursorContext::ProviderContext` — show provider name, supported models list,
features. Use `nika_core::catalogs::providers::KNOWN_PROVIDERS` for data.

```rust
fn provider_hover(prefix: &str, current_provider: Option<&str>) -> Option<HoverResult> {
    let provider_name = current_provider.or_else(|| {
        // Try to match prefix against known providers
        KNOWN_PROVIDERS.iter()
            .find(|p| p.id.starts_with(prefix.trim()))
            .map(|p| p.id)
    })?;

    let provider = KNOWN_PROVIDERS.iter().find(|p| p.id == provider_name)?;

    let models: Vec<&str> = KNOWN_MODELS.iter()
        .filter(|m| m.provider == provider_name)
        .map(|m| m.id)
        .collect();

    let markdown = format!(
        "## `{}` — {}\n\n**Category:** {:?}\n\n**Models ({}):**\n{}",
        provider.id, provider.display_name, provider.category,
        models.len(),
        models.iter().map(|m| format!("- `{m}`")).collect::<Vec<_>>().join("\n"),
    );

    Some(HoverResult { contents: markdown, range: None })
}
```

**Verification:**
- Test: hover on `provider: claude` shows Claude provider info with model list
- Test: hover on `provider: unknown` returns None

#### Task C1.2: Add model hover

**File:** `tools/nika-lsp-core/src/handlers/hover.rs` (append ~60 LOC)

When cursor is on a model name (inside `ProviderContext` or `VerbBlock` where prefix
matches a known model), show model info: provider, type (chat/embedding/vision), context
window.

```rust
fn model_hover(model_id: &str) -> Option<HoverResult> {
    let model = KNOWN_MODELS.iter().find(|m| m.id == model_id)?;
    let markdown = format!(
        "## `{}`\n\n**Provider:** `{}`\n**Type:** {:?}\n**Context:** {} tokens",
        model.id, model.provider, model.model_type, model.context_window,
    );
    Some(HoverResult { contents: markdown, range: None })
}
```

**Verification:**
- Test: `model_hover("claude-sonnet-4-6")` returns provider and context window
- Test: `model_hover("nonexistent")` returns None

#### Task C1.3: Add template variable hover

**File:** `tools/nika-lsp-core/src/handlers/hover.rs` (append ~60 LOC)

When hovering over `{{with.alias}}`, show which task it's bound to and what output
type to expect.

```rust
fn template_variable_hover(text: &str, partial_expr: &str) -> Option<HoverResult> {
    if let Some(alias) = partial_expr.strip_prefix("with.") {
        let alias = alias.split('.').next().unwrap_or("");
        // Find the binding: look for `alias: $task_id` in with: blocks
        let pat = format!("{alias}: $");
        if let Some(pos) = text.find(&pat) {
            let after = &text[pos + pat.len()..];
            let task_ref: String = after.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            return Some(HoverResult {
                contents: format!(
                    "## `with.{alias}`\n\nBound to output of task `{task_ref}`."
                ),
                range: None,
            });
        }
    }
    None
}
```

**Verification:**
- Test: given text with `with: { data: $step1 }`, hover on `with.data` shows "Bound to step1"

#### Task C1.4: Add task ID hover

**File:** `tools/nika-lsp-core/src/handlers/hover.rs` (append ~50 LOC)

When hovering on a task ID in `depends_on:`, show the task's verb and description.

**Verification:**
- Test: hover on task ID in `depends_on: [step1]` shows step1's verb type

#### Task C1.5: Wire new hover functions into main dispatch

**File:** `tools/nika-lsp-core/src/handlers/hover.rs` (modify `hover()` fn at line 10)

```rust
pub fn hover(text: &str, _offset: u32, context: &CursorContext) -> Option<HoverResult> {
    match context {
        CursorContext::VerbBlock { verb, .. } => verb_hover(verb),
        CursorContext::TaskField { prefix, .. } => field_hover(prefix),
        CursorContext::WorkflowRoot { prefix } => root_key_hover(prefix),
        CursorContext::ContentPart { focus, .. } => content_hover(focus),
        CursorContext::WithBlock { .. } => with_block_hover(),
        CursorContext::ProviderContext { prefix, current_provider, .. } =>
            provider_hover(prefix, current_provider.as_deref())
                .or_else(|| model_hover(prefix.trim())),
        CursorContext::Template { partial_expr, in_transform_chain: true, .. } =>
            transform_hover(partial_expr),
        CursorContext::Template { partial_expr, .. } =>
            template_variable_hover(text, partial_expr),
        CursorContext::DependsOn { prefix, .. } => task_id_hover(text, prefix),
        _ => None,
    }
}
```

**Verification:**
- All existing hover tests still pass
- 5 new test cases for the new hover paths

---

**PR-C1 Summary:**

| Item | Value |
|------|-------|
| Files modified | `hover.rs` |
| Estimated LOC | +350 (86 -> ~440) |
| Tests required | 10 (5 existing + 5 new paths) |
| Dependencies | None |
| Estimated time | 2 hours |

---

### PR-C2: Port Definition (58 LOC -> ~400 LOC)

**Goal:** The current core definition (58 LOC) handles `DependsOn` and `WithBlock` via
text search. The embedded handler (946 LOC) also covers: template variable -> with binding
-> task definition (chained), include path -> file, AST-based span-accurate task lookup,
cross-file references.

**Depends on:** None

#### Task C2.1: Add include path definition

**File:** `tools/nika-lsp-core/src/handlers/definition.rs` (append ~60 LOC)

When cursor is on a path in `include:`, resolve the path relative to the document.

```rust
fn find_include_def(text: &str, offset: u32) -> Option<DefinitionResult> {
    // Find line containing offset
    let line = get_line_at_offset(text, offset);
    let trimmed = line.trim();

    // Match: `- path: ./partial.nika.yaml` or `path: ./file.nika.yaml`
    if let Some(path_val) = trimmed.strip_prefix("- path:")
        .or_else(|| trimmed.strip_prefix("path:"))
    {
        let path = path_val.trim().trim_matches('"').trim_matches('\'');
        if !path.is_empty() {
            return Some(DefinitionResult {
                offset: 0,
                end_offset: 0,
                file: Some(path.to_string()),
            });
        }
    }
    None
}
```

**Verification:**
- Test: `include: - path: ./partial.nika.yaml` returns file path

#### Task C2.2: Add chained template -> binding -> task definition

**File:** `tools/nika-lsp-core/src/handlers/definition.rs` (line 11-18)

Improve the `Template` branch to chain through `with:` bindings:

```rust
CursorContext::Template { partial_expr, .. } => {
    if let Some(rest) = partial_expr.strip_prefix("with.") {
        let alias = rest.split('.').next().unwrap_or("");
        // First: go to the with: binding line itself
        find_with_binding_line(text, alias)
            // If not found, try resolving through to the task
            .or_else(|| find_with_source(text, alias))
    } else if let Some(rest) = partial_expr.strip_prefix("context.files.") {
        find_context_file_def(text, rest)
    } else {
        None
    }
}
```

**Verification:**
- Test: `{{with.data}}` with `data: $step1` navigates to `- id: step1`
- Test: `{{context.files.readme}}` navigates to context definition

#### Task C2.3: Add helper `get_line_at_offset`

**File:** `tools/nika-lsp-core/src/handlers/definition.rs` (append ~15 LOC)

```rust
fn get_line_at_offset(text: &str, offset: u32) -> &str {
    let start = text[..offset as usize].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let end = text[offset as usize..].find('\n')
        .map(|p| offset as usize + p)
        .unwrap_or(text.len());
    &text[start..end]
}
```

**Verification:**
- Test: first line, middle line, last line, empty text

#### Task C2.4: Add `find_with_binding_line` for direct binding navigation

**File:** `tools/nika-lsp-core/src/handlers/definition.rs` (append ~25 LOC)

Navigate to the actual `alias: $task_id` line in a `with:` block, not the task itself.
This gives a two-hop experience: first jump to binding, then jump from binding to task.

```rust
fn find_with_binding_line(text: &str, alias: &str) -> Option<DefinitionResult> {
    let patterns = [
        format!("{alias}: $"),
        format!("{alias}: \"$"),
    ];
    for pat in &patterns {
        if let Some(pos) = text.find(pat) {
            return Some(DefinitionResult {
                offset: pos as u32,
                end_offset: (pos + pat.len()) as u32,
                file: None,
            });
        }
    }
    None
}
```

**Verification:**
- Test: finds `data: $step1` in a with block

---

**PR-C2 Summary:**

| Item | Value |
|------|-------|
| Files modified | `definition.rs` |
| Estimated LOC | +200 (58 -> ~260) |
| Tests required | 8 |
| Dependencies | None |
| Estimated time | 1.5 hours |

---

### PR-C3: Port Symbols (109 LOC -> ~400 LOC)

**Goal:** The current core symbols (109 LOC) extracts tasks and MCP servers as flat
symbols. The embedded handler (825 LOC) also provides: hierarchical task structure with
verb children, `with:` bindings as child symbols, `content:` parts, proper symbol kinds,
selection ranges matching the task's full span, detail strings (verb type, dependency count).

**Depends on:** None

#### Task C3.1: Add verb children to task symbols

**File:** `tools/nika-lsp-core/src/handlers/symbols.rs` (enhance `extract_tasks`)

The current implementation already extracts verb children (line 48-50) but as simple
single-line entries. Enhance to capture the full verb block span:

```rust
// Track verb block end by looking ahead for next sibling field
let verb_end = find_block_end(lines, j + 1, ci);
children.push(SymbolEntry {
    name: format!("{v}:"),
    kind: SymbolKind::Function,
    offset: co as u32,
    end_offset: verb_end as u32,
    children: extract_verb_subfields(lines, j + 1, ci + 2, text),
});
```

**Verification:**
- Test: task with multi-line `infer:` block has verb child spanning all lines

#### Task C3.2: Add detail strings

**File:** `tools/nika-lsp-core/src/handlers/symbols.rs`

Add a `detail` field to `SymbolEntry`:

```diff
 pub struct SymbolEntry {
     pub name: String,
     pub kind: SymbolKind,
+    pub detail: Option<String>,
     pub offset: u32,
     pub end_offset: u32,
     pub children: Vec<SymbolEntry>,
 }
```

Populate with verb type and dependency count:
- Task: `detail: Some("infer | 2 deps".to_string())`
- MCP server: `detail: Some("command: npx".to_string())`

**Verification:**
- Test: task symbol has detail showing verb type

#### Task C3.3: Add context, inputs, edges sections

**File:** `tools/nika-lsp-core/src/handlers/symbols.rs` (append ~60 LOC)

Extract `context:`, `inputs:`, `edges:` as top-level symbols with children.

**Verification:**
- Test: workflow with `inputs:` section shows in outline

#### Task C3.4: Add `for_each` and `content` as task children

**File:** `tools/nika-lsp-core/src/handlers/symbols.rs` (enhance `extract_tasks`)

When a task has `for_each:` or `content:`, add them as child symbols.

**Verification:**
- Test: task with `for_each: [1,2,3]` shows for_each child
- Test: task with `content:` block shows content child with parts

---

**PR-C3 Summary:**

| Item | Value |
|------|-------|
| Files modified | `symbols.rs` |
| Estimated LOC | +200 (109 -> ~310) |
| Tests required | 8 |
| Dependencies | None |
| Estimated time | 1.5 hours |

---

### PR-C4: Port Code Action (44 LOC -> ~500 LOC)

**Goal:** The current core code_action (44 LOC) has 2 actions: add schema, expand infer.
The embedded handler (1,135 LOC) has 7: fix unknown task (fuzzy match), add missing `id:`,
add `depends_on:`, convert shorthand to full form (all verbs), add provider, fix schema
version, extract task to include.

**Depends on:** None

#### Task C4.1: Add "fix unknown task" action

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (append ~80 LOC)

When text references a task ID that does not exist, suggest the closest match.
Uses Levenshtein distance (inline, no external dep):

```rust
fn fix_unknown_task(text: &str, bad_name: &str, offset: u32) -> Option<CodeActionEntry> {
    let task_ids = extract_task_ids_from_text(text);
    let best = task_ids.iter()
        .map(|id| (id, levenshtein(bad_name, id)))
        .filter(|(_, d)| *d <= 3)
        .min_by_key(|(_, d)| *d)?;

    Some(CodeActionEntry {
        title: format!("Did you mean '{}'?", best.0),
        kind: CodeActionKind::QuickFix,
        is_preferred: true,
        edit: Some(TextEdit {
            offset,
            end_offset: offset + bad_name.len() as u32,
            new_text: best.0.clone(),
        }),
    })
}
```

**Verification:**
- Test: `depends_on: [step_1]` with task `step-1` suggests correction

#### Task C4.2: Add "add missing id" action

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (append ~30 LOC)

When a task has a verb but no `id:`, offer to add one.

**Verification:**
- Test: task with `infer: "hi"` but no `id:` gets "Add task ID" action

#### Task C4.3: Add "add depends_on" action

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (append ~30 LOC)

When a task uses `with: { data: $step1 }` but has no `depends_on:`, offer to add it.

**Verification:**
- Test: task with `with` binding but no `depends_on` gets suggestion

#### Task C4.4: Add "expand shorthand" for all verbs

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (line 22-31)

Generalize the existing `infer:` expand to all 5 verbs:

```rust
for verb in ["infer", "exec", "fetch", "invoke", "agent"] {
    if let Some(rest) = trimmed.strip_prefix(&format!("{verb}:")) {
        let value = rest.trim().trim_matches('"').trim_matches('\'');
        if !value.is_empty() && !value.contains('\n') {
            let main_field = match verb {
                "infer" | "agent" => "prompt",
                "exec" => "command",
                "fetch" => "url",
                "invoke" => "tool",
                _ => continue,
            };
            actions.push(CodeActionEntry {
                title: format!("Expand shorthand {verb}"),
                kind: CodeActionKind::Refactor,
                is_preferred: false,
                edit: Some(TextEdit {
                    offset: line_start as u32,
                    end_offset: line_end as u32,
                    new_text: format!(
                        "{indent}{verb}:\n{indent}  {main_field}: |\n{indent}    {value}"
                    ),
                }),
            });
        }
    }
}
```

**Verification:**
- Test: `exec: "ls -la"` gets "Expand shorthand exec" action
- Test: `fetch: "https://api.example.com"` gets "Expand shorthand fetch" action

#### Task C4.5: Add "add provider" action

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (append ~30 LOC)

When a task has `infer:` or `agent:` but no `provider:`, offer to add one.

**Verification:**
- Test: infer task without provider gets "Add provider" with provider list

#### Task C4.6: Add Levenshtein helper

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (append ~25 LOC)

Inline Levenshtein distance (no external dep, 25 LOC):

```rust
fn levenshtein(a: &str, b: &str) -> usize {
    let (m, n) = (a.len(), b.len());
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j] + cost)
                .min(prev[j + 1] + 1)
                .min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
```

**Verification:**
- Test: `levenshtein("step1", "step_1") == 1`
- Test: `levenshtein("abc", "abc") == 0`
- Test: `levenshtein("", "abc") == 3`

---

**PR-C4 Summary:**

| Item | Value |
|------|-------|
| Files modified | `code_action.rs` |
| Estimated LOC | +350 (44 -> ~400) |
| Tests required | 12 |
| Dependencies | None |
| Estimated time | 2 hours |

---

## Track D: Novel AI-Specific Features (3 PRs)

These features have no equivalent in any competing workflow-engine LSP. They represent
Nika's differentiation: understanding AI-specific concerns at authoring time.

### PR-D1: Cost Radar (Show $/run Estimates Inline)

**Goal:** Show estimated cost per task and total workflow cost as inlay hints.
Example: `infer: "Summarize" # ~$0.003/run` appears as a faded hint after the line.

**Depends on:** PR-A2 (embedded wiring), nika_core::catalogs for model data

#### Task D1.1: Create cost estimation module

**File:** `tools/nika-lsp-core/src/knowledge/cost.rs` (NEW — ~200 LOC)

**File:** `tools/nika-lsp-core/src/knowledge/mod.rs` (NEW — ~10 LOC)

```rust
pub mod cost;
```

```rust
// cost.rs

use nika_core::catalogs::models::KNOWN_MODELS;

/// Estimated cost for a single task invocation.
#[derive(Debug, Clone)]
pub struct CostEstimate {
    pub task_id: String,
    pub provider: String,
    pub model: String,
    pub input_tokens_estimate: u32,
    pub output_tokens_estimate: u32,
    pub cost_usd: f64,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Copy)]
pub enum Confidence { High, Medium, Low }

/// Estimate cost for all tasks in a workflow.
pub fn estimate_workflow_cost(text: &str) -> Vec<CostEstimate> {
    // 1. Parse tasks (reuse extract_task_ids + verb detection)
    // 2. For each infer/agent task:
    //    a. Detect provider/model (explicit or default)
    //    b. Estimate input tokens from prompt length (~4 chars/token)
    //    c. Estimate output tokens (default 500, or from max_tokens)
    //    d. Look up pricing from KNOWN_MODELS
    //    e. Calculate: (input * input_price + output * output_price) / 1M
    // 3. For fetch/exec/invoke: cost = $0.00
    vec![]
}
```

**Verification:**
- Test: workflow with `infer: "short prompt"` + `model: claude-sonnet-4-6` estimates ~$0.002
- Test: workflow with only `exec:` tasks estimates $0.00
- Test: unknown model returns Low confidence

#### Task D1.2: Create inlay hints handler

**File:** `tools/nika-lsp-core/src/handlers/inlay_hints.rs` (NEW — ~120 LOC)

```rust
#[derive(Debug, Clone)]
pub struct InlayHintEntry {
    pub offset: u32,         // End of the task's verb line
    pub label: String,       // "~$0.003/run"
    pub kind: InlayKind,
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum InlayKind { Cost, TokenCount, ModelInfo }

/// Generate inlay hints for cost estimates.
pub fn inlay_hints(text: &str, start: u32, end: u32) -> Vec<InlayHintEntry> {
    let estimates = crate::knowledge::cost::estimate_workflow_cost(text);
    let mut hints = Vec::new();

    for est in &estimates {
        if est.cost_usd > 0.0 {
            hints.push(InlayHintEntry {
                offset: find_task_verb_end(text, &est.task_id),
                label: format!("~${:.4}/run", est.cost_usd),
                kind: InlayKind::Cost,
                tooltip: Some(format!(
                    "{} on {} | ~{} in / ~{} out tokens | {} confidence",
                    est.model, est.provider,
                    est.input_tokens_estimate, est.output_tokens_estimate,
                    match est.confidence {
                        Confidence::High => "high",
                        Confidence::Medium => "medium",
                        Confidence::Low => "low",
                    }
                )),
            });
        }
    }

    // Total workflow cost at bottom
    let total: f64 = estimates.iter().map(|e| e.cost_usd).sum();
    if total > 0.0 {
        hints.push(InlayHintEntry {
            offset: text.len() as u32,
            label: format!("Total: ~${:.4}/run", total),
            kind: InlayKind::Cost,
            tooltip: Some(format!("{} tasks, {} with LLM calls", estimates.len(),
                estimates.iter().filter(|e| e.cost_usd > 0.0).count())),
        });
    }

    hints
}
```

**File:** `tools/nika-lsp-core/src/handlers/mod.rs`

```diff
 pub mod code_action;
 pub mod completion;
 pub mod definition;
 pub mod diagnostics;
 pub mod hover;
+pub mod inlay_hints;
 pub mod semantic_tokens;
 pub mod symbols;
```

**Verification:**
- Test: 3-task workflow with 2 infer tasks shows 2 cost hints + 1 total
- Test: empty workflow shows no hints
- Test: fetch-only workflow shows no cost hints

#### Task D1.3: Wire inlay hints in embedded LSP

**File:** `tools/nika/src/lsp/server.rs`

Register `inlay_hint_provider` in capabilities and implement:

```rust
async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
    let uri = &params.text_document.uri;
    let docs = self.documents.read().await;
    let text = docs.get(uri).cloned().unwrap_or_default();

    let start = super::conversion::position_to_offset(params.range.start, &text) as u32;
    let end = super::conversion::position_to_offset(params.range.end, &text) as u32;

    let hints = nika_lsp_core::handlers::inlay_hints::inlay_hints(&text, start, end);
    let lsp_hints: Vec<InlayHint> = hints.into_iter()
        .map(|h| convert_inlay_hint(h, &text))
        .collect();

    Ok(Some(lsp_hints))
}
```

**Verification:**
- `cargo test --lib -p nika --features lsp` passes
- Manual: VS Code shows `~$0.003/run` after infer tasks

#### Task D1.4: Wire inlay hints in standalone LSP

**File:** `tools/nika-lsp/src/backend.rs`

Same as D1.3 but for standalone.

**Verification:**
- Standalone server shows same cost hints

---

**PR-D1 Summary:**

| Item | Value |
|------|-------|
| Files created | `knowledge/mod.rs`, `knowledge/cost.rs`, `handlers/inlay_hints.rs` |
| Files modified | `handlers/mod.rs`, `lib.rs`, `server.rs`, `backend.rs` |
| Estimated LOC | +450 |
| Tests required | 10 |
| Dependencies | PR-A2 (for embedded wiring), PR-A3 (for standalone wiring) |
| Estimated time | 3 hours |

---

### PR-D2: Prompt Linting (Detect Common Prompt Engineering Mistakes)

**Goal:** Add diagnostics that flag common prompt engineering anti-patterns. These are
warnings, not errors — they appear as yellow squiggles under prompt text.

**Depends on:** PR-A1 (diagnostics type), PR-C1 (for hover integration)

#### Task D2.1: Define prompt lint rules

**File:** `tools/nika-lsp-core/src/knowledge/prompt_lint.rs` (NEW — ~300 LOC)

```rust
/// A prompt lint finding.
#[derive(Debug, Clone)]
pub struct PromptLint {
    pub rule: PromptRule,
    pub offset: u32,
    pub end_offset: u32,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptRule {
    /// Prompt is too short (<10 chars) — likely incomplete.
    TooShort,
    /// Prompt has no instruction verb (analyze, summarize, generate, etc.).
    NoActionVerb,
    /// Prompt uses "do not" instead of affirmative instruction.
    NegativeInstruction,
    /// Prompt mixes languages (detected via simple heuristic).
    MixedLanguage,
    /// System prompt duplicates the task prompt.
    DuplicatePrompt,
    /// Prompt references undefined template variable.
    UndefinedVariable,
    /// Temperature > 1.5 (likely mistake).
    HighTemperature,
    /// max_tokens < 50 (often too restrictive).
    LowMaxTokens,
    /// Using vision model without content: block.
    VisionModelNoContent,
    /// Using non-vision model with content: block.
    NonVisionModelWithContent,
}

impl PromptRule {
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooShort => "NIKA-LINT-001",
            Self::NoActionVerb => "NIKA-LINT-002",
            Self::NegativeInstruction => "NIKA-LINT-003",
            Self::MixedLanguage => "NIKA-LINT-004",
            Self::DuplicatePrompt => "NIKA-LINT-005",
            Self::UndefinedVariable => "NIKA-LINT-006",
            Self::HighTemperature => "NIKA-LINT-007",
            Self::LowMaxTokens => "NIKA-LINT-008",
            Self::VisionModelNoContent => "NIKA-LINT-009",
            Self::NonVisionModelWithContent => "NIKA-LINT-010",
        }
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        match self {
            Self::UndefinedVariable => DiagnosticSeverity::Error,
            Self::HighTemperature | Self::NonVisionModelWithContent
                => DiagnosticSeverity::Warning,
            _ => DiagnosticSeverity::Info,
        }
    }
}

/// Run all prompt lint rules against a workflow.
pub fn lint_prompts(text: &str) -> Vec<PromptLint> {
    let mut lints = Vec::new();
    // Iterate over tasks, find prompt: values, run rules
    lint_short_prompts(text, &mut lints);
    lint_no_action_verb(text, &mut lints);
    lint_negative_instructions(text, &mut lints);
    lint_high_temperature(text, &mut lints);
    lint_low_max_tokens(text, &mut lints);
    lint_vision_mismatch(text, &mut lints);
    lints
}
```

**Verification:**
- Test: `prompt: "hi"` triggers TooShort
- Test: `prompt: "Summarize this article"` does NOT trigger NoActionVerb
- Test: `prompt: "Do not include headers"` triggers NegativeInstruction
- Test: `temperature: 2.5` triggers HighTemperature

#### Task D2.2: Wire lints into diagnostics pipeline

**File:** `tools/nika-lsp-core/src/handlers/diagnostics.rs`

```rust
pub fn diagnostics(text: &str) -> Vec<DiagnosticEntry> {
    let mut entries = Vec::new();

    // Prompt lints
    for lint in crate::knowledge::prompt_lint::lint_prompts(text) {
        entries.push(DiagnosticEntry {
            offset: lint.offset,
            end_offset: lint.end_offset,
            severity: lint.rule.severity(),
            code: lint.rule.code().to_string(),
            message: lint.message,
            source: "nika",
        });
    }

    entries
}
```

**Verification:**
- Test: `diagnostics("tasks:\n  - id: t\n    infer: \"hi\"")` returns TooShort lint

#### Task D2.3: Add quick-fix actions for lint findings

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (append ~60 LOC)

For NegativeInstruction: suggest rephrasing ("Do not include" -> "Exclude").
For HighTemperature: suggest `temperature: 0.7`.

**Verification:**
- Test: NegativeInstruction lint generates quick-fix action

---

**PR-D2 Summary:**

| Item | Value |
|------|-------|
| Files created | `knowledge/prompt_lint.rs` |
| Files modified | `knowledge/mod.rs`, `handlers/diagnostics.rs`, `handlers/code_action.rs` |
| Estimated LOC | +400 |
| Tests required | 12 |
| Dependencies | PR-A1 (DiagnosticEntry type) |
| Estimated time | 3 hours |

---

### PR-D3: Model Switching (Quick-Fix to Swap Providers/Models)

**Goal:** When the cursor is on a `model:` or `provider:` field, offer code actions to
switch to alternative models with a preview of the cost/capability trade-off.

**Depends on:** PR-C1 (hover with model info), PR-D1 (cost estimation)

#### Task D3.1: Create model alternatives finder

**File:** `tools/nika-lsp-core/src/knowledge/model_switch.rs` (NEW — ~150 LOC)

```rust
/// Suggested model alternative.
#[derive(Debug, Clone)]
pub struct ModelAlternative {
    pub model_id: String,
    pub provider: String,
    pub reason: String,          // "50% cheaper", "supports vision", "larger context"
    pub cost_delta: Option<f64>, // Negative = cheaper
}

/// Find alternatives for the current model.
pub fn alternatives_for(current_model: &str) -> Vec<ModelAlternative> {
    let current = KNOWN_MODELS.iter().find(|m| m.id == current_model);
    let current = match current { Some(c) => c, None => return vec![] };

    let mut alts = Vec::new();

    // Same provider, different tier
    for m in KNOWN_MODELS.iter() {
        if m.provider == current.provider && m.id != current.id
            && m.model_type == current.model_type {
            alts.push(ModelAlternative {
                model_id: m.id.to_string(),
                provider: m.provider.to_string(),
                reason: compare_models(current, m),
                cost_delta: None, // TODO: wire cost comparison
            });
        }
    }

    // Cross-provider equivalent
    for m in KNOWN_MODELS.iter() {
        if m.provider != current.provider && m.model_type == current.model_type {
            alts.push(ModelAlternative {
                model_id: m.id.to_string(),
                provider: m.provider.to_string(),
                reason: format!("Alternative on {}", m.provider),
                cost_delta: None,
            });
        }
    }

    // Limit to top 5
    alts.truncate(5);
    alts
}
```

**Verification:**
- Test: `alternatives_for("claude-sonnet-4-6")` includes opus and haiku
- Test: `alternatives_for("unknown-model")` returns empty

#### Task D3.2: Create model switch code actions

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (append ~80 LOC)

Detect when cursor range covers a `model:` line and offer alternatives:

```rust
fn model_switch_actions(text: &str, start: u32, end: u32) -> Vec<CodeActionEntry> {
    let line = get_line_at_offset(text, start);
    let trimmed = line.trim();

    if let Some(model_val) = trimmed.strip_prefix("model:") {
        let model = model_val.trim().trim_matches('"').trim_matches('\'');
        let line_start = text[..start as usize].rfind('\n')
            .map(|p| p + 1).unwrap_or(0);
        let line_end = text[start as usize..].find('\n')
            .map(|p| start as usize + p).unwrap_or(text.len());
        let indent = &line[..line.len() - trimmed.len()];

        return crate::knowledge::model_switch::alternatives_for(model)
            .into_iter()
            .map(|alt| CodeActionEntry {
                title: format!("Switch to {} ({})", alt.model_id, alt.reason),
                kind: CodeActionKind::Refactor,
                is_preferred: false,
                edit: Some(TextEdit {
                    offset: line_start as u32,
                    end_offset: line_end as u32,
                    new_text: format!("{indent}model: {}", alt.model_id),
                }),
            })
            .collect();
    }

    vec![]
}
```

**Verification:**
- Test: cursor on `model: claude-sonnet-4-6` shows 5 alternatives
- Test: cursor on `model: unknown` shows no alternatives
- Test: cursor on `prompt: "hi"` shows no model actions

#### Task D3.3: Wire into main code_actions dispatch

**File:** `tools/nika-lsp-core/src/handlers/code_action.rs` (inside `code_actions` fn)

```rust
pub fn code_actions(text: &str, start_offset: u32, end_offset: u32)
    -> Vec<CodeActionEntry>
{
    let mut actions = Vec::new();
    // ... existing schema + expand actions ...
    actions.extend(model_switch_actions(text, start_offset, end_offset));
    actions
}
```

**Verification:**
- Test: full code_actions returns both schema action and model switch actions

---

**PR-D3 Summary:**

| Item | Value |
|------|-------|
| Files created | `knowledge/model_switch.rs` |
| Files modified | `knowledge/mod.rs`, `handlers/code_action.rs` |
| Estimated LOC | +280 |
| Tests required | 8 |
| Dependencies | PR-D1 (cost module for delta), PR-C1 (model hover) |
| Estimated time | 2 hours |

---

## Dependency Graph

```
                        ┌─────────┐
                        │  PR-A1  │  Unified handler trait
                        └────┬────┘
                       ┌─────┼──────┐
                       │     │      │
                  ┌────▼──┐  │  ┌───▼───┐
                  │ PR-A2 │  │  │ PR-A3 │  Wire embedded / standalone
                  └────┬──┘  │  └───┬───┘
                       │     │      │
                       └─────┼──────┘
                             │
     ┌───────────────────────┼───────────────────────┐
     │                       │                       │
 ┌───▼───┐             ┌────▼────┐             ┌────▼────┐
 │ PR-D1 │             │ PR-D2  │             │ PR-D3  │
 │ Cost  │             │ Lint   │             │ Switch │
 └───────┘             └────────┘             └────────┘

 Independent (can start immediately):
 ┌───────┐  ┌───────┐  ┌───────┐  ┌───────┐  ┌───────┐  ┌───────┐
 │ PR-B1 │  │ PR-B2 │  │ PR-C1 │  │ PR-C2 │  │ PR-C3 │  │ PR-C4 │
 │ Fetch │  │ Agent │  │ Hover │  │ Defn  │  │ Syms  │  │ Action│
 └───────┘  └───────┘  └───────┘  └───────┘  └───────┘  └───────┘
```

**Critical path:** PR-A1 -> PR-A2 + PR-A3 (parallel) -> PR-D1/D2/D3

**Maximum parallelism:** All 6 Track B + C PRs can start immediately. Only Track D
requires Track A completion for wiring.

---

## Execution Order (Recommended)

| Session | PRs | Parallelism | Estimated Time |
|:-------:|-----|:-----------:|:--------------:|
| 1 | PR-A1 + PR-B1 + PR-B2 | 3 parallel | 2h |
| 2 | PR-C1 + PR-C2 | 2 parallel | 2h |
| 3 | PR-C3 + PR-C4 | 2 parallel | 2h |
| 4 | PR-A2 + PR-A3 | 2 parallel | 2h |
| 5 | PR-D1 | 1 (depends on A2/A3) | 3h |
| 6 | PR-D2 | 1 (depends on A1) | 3h |
| 7 | PR-D3 | 1 (depends on D1, C1) | 2h |

**Total: 7 sessions, ~16 hours of implementation.**

---

## Test Requirements Summary

| PR | New Tests | Cumulative nika-lsp-core Tests |
|----|:---------:|:------------------------------:|
| PR-A1 | 4 | 277 |
| PR-B1 | 6 | 283 |
| PR-B2 | 8 | 291 |
| PR-C1 | 10 | 301 |
| PR-C2 | 8 | 309 |
| PR-C3 | 8 | 317 |
| PR-C4 | 12 | 329 |
| PR-A2 | 6 | 329 (nika tests) |
| PR-A3 | 5 | 329 (nika-lsp tests) |
| PR-D1 | 10 | 339 |
| PR-D2 | 12 | 351 |
| PR-D3 | 8 | 359 |
| **Total** | **97** | **359+** |

All tests use `cargo test --lib` to avoid Keychain popups.

---

## LOC Impact Summary

| Action | LOC |
|--------|:---:|
| New code in nika-lsp-core | +2,190 |
| New code in nika (wiring) | +200 |
| New code in nika-lsp (wiring) | +50 |
| **Total new** | **+2,440** |

After Track A completes, the follow-up "delete old handlers" PR removes ~10,670 LOC
of duplicated code, for a net reduction of ~8,230 LOC across the workspace.

---

## Success Criteria

- [ ] All 6 handlers routed through nika-lsp-core in both binaries
- [ ] Completion includes v0.35 fetch/agent/infer fields
- [ ] Hover shows provider, model, template, and task info
- [ ] Definition chains through templates and includes
- [ ] Symbols show full task hierarchy with verb children
- [ ] Code actions include fuzzy task fix, expand all verbs, model switch
- [ ] Cost radar shows $/run inline for all infer/agent tasks
- [ ] Prompt linter flags 10 anti-patterns
- [ ] 359+ tests in nika-lsp-core (up from 273)
- [ ] Zero code duplication between embedded and standalone for the 6 core handlers

---

## Risk Mitigations

| Risk | Mitigation |
|------|------------|
| Core handler returns fewer results than legacy | Keep fallback during Track A; delete legacy only after 1 week of dual-write |
| Cost estimation inaccurate | Mark all estimates with confidence level; show "~" prefix |
| Prompt lint false positives | All lint rules are Info severity by default; users can disable |
| Model catalog stale | Static catalog + quarterly update cadence; document in CLAUDE.md |
| PR-A2/A3 break existing users | Feature-flag the delegation behind `cfg(feature = "lsp-core-delegation")`; enable by default after testing |
