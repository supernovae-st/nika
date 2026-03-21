# Research Report: Building a Code Editor in a Ratatui TUI

**Date:** 2026-03-21
**Context:** Improving Nika's Studio View YAML workflow editor
**Current stack:** ratatui 0.30, crossterm 0.29, tree-sitter + tree-sitter-yaml, tui-input 0.15, nucleo 0.5

---

## Summary

The ratatui ecosystem has matured significantly for code editing. Three viable paths exist: (1) adopt **edtui** for a battle-tested Vim-modal editor widget, (2) adopt **ratatui-textarea** for a VS Code-like multi-line editor, or (3) continue building a custom editor using Nika's existing `TextBuffer` + tree-sitter highlighter + `SelectionSet` + `EditHistory` foundation. For Nika's YAML-specific needs (schema validation, Nika verb awareness, template highlighting, LSP integration), option 3 with selective borrowing from edtui/ratatui-textarea is the recommended path.

---

## Key Findings

### 1. Multi-Line Editor Widgets for Ratatui

#### edtui (Recommended for Vim-modal editing)

- **Crate:** [edtui](https://crates.io/crates/edtui) | [GitHub](https://github.com/preiter93/edtui)
- **Version:** 0.11.2 (2026-03-08) -- actively maintained
- **Downloads:** 92,437 total / 44,783 recent
- **Features:**
  - Vim-like modes: Normal, Insert, Visual
  - Clipboard integration (arboard -- Nika already uses arboard)
  - Mouse support
  - Built-in syntax highlighting
  - Undo/redo
- **Fit for Nika:** Good modal editing foundation, but Nika already has its own EditorMode (Normal/Insert), SelectionSet, and EditHistory. Adopting edtui would mean replacing substantial custom code. The syntax highlighting is generic -- Nika's tree-sitter highlighter with Nika-specific captures (Verb, TaskId, Template, McpServer) is more valuable.
- **Verdict:** Study its keybinding patterns and text manipulation internals. Do not adopt wholesale -- Nika's domain-specific features would be lost.

#### ratatui-textarea (Recommended for VS Code-like editing)

- **Crate:** [ratatui-textarea](https://crates.io/crates/ratatui-textarea) | [GitHub](https://github.com/ratatui/ratatui-textarea)
- **Version:** 0.8.0 (2026-02-21) -- maintained by ratatui org
- **Downloads:** 20,050 total / 8,656 recent
- **Lineage:** Hard fork of tui-textarea (rhysd), maintained by ratatui org for ratatui 0.30+ compatibility
- **Features:**
  - Multi-line editing with cursor movement
  - Undo/redo
  - Line numbers
  - Cursor line highlighting
  - Unicode-aware soft word wrap
  - Regex search
  - Text selection with highlighted ranges
  - Custom syntax highlighting hook
- **Fit for Nika:** The Studio view comment says this is the planned direction. The 0.8.0 release aligns with ratatui 0.30. Could replace the custom TextBuffer while keeping Nika's tree-sitter highlighter as the syntax highlighting provider.
- **Verdict:** Strong candidate for the editing core. Plug Nika's TreeSitterHighlighter into its custom highlighting hook. However, it lacks floating popups, LSP integration, and diagnostic rendering -- those remain custom work.

#### tui-textarea (Legacy, avoid for new code)

- **Crate:** [tui-textarea](https://crates.io/crates/tui-textarea) | [GitHub](https://github.com/rhysd/tui-textarea)
- **Version:** 0.7.0 (2024-10-22) -- last update 17 months ago
- **Downloads:** 1,258,716 total / 359,695 recent (high due to legacy adoption)
- **Verdict:** Use ratatui-textarea instead. The ratatui org fork is the maintained path forward.

#### ratatui-code-editor (Too early, watch)

- **Crate:** [ratatui-code-editor](https://crates.io/crates/ratatui-code-editor) | [GitHub](https://github.com/vipmax/ratatui-code-editor)
- **Version:** 0.0.3 (2026-02-28)
- **Downloads:** 638 total / 164 recent
- **Features:** Tree-sitter syntax highlighting, basic editing
- **Verdict:** Pre-alpha. Only 638 downloads. No docs on line numbers, scrolling, selection, undo/redo. Not production-ready. Monitor but do not adopt.

### 2. Syntax Highlighting Integration

#### Nika's Current Approach (Keep and extend)

Nika already has a production-quality tree-sitter highlighter at `tui/highlight/`:
- `TreeSitterHighlighter` with incremental parsing support
- Nika-specific captures: Verb, TaskId, Template, McpServer (beyond what any generic crate offers)
- Solarized theme with domain-aware coloring

This is more advanced than what any off-the-shelf crate provides for YAML workflows.

#### tui-syntax (Niche, not needed)

- **Crate:** [tui-syntax](https://crates.io/crates/tui-syntax) | [GitHub](https://github.com/fcoury/tsql)
- **Version:** 0.5.0 (2026-03-02)
- **Downloads:** 400 total
- Helix-compatible TOML theme format, tree-sitter based
- **Verdict:** Nika's highlighter is already superior for its use case. No value in switching.

#### syntect-tui (Alternative approach, not recommended)

- Regex-based (syntect) rather than tree-sitter
- Simpler but less accurate for complex grammars
- **Verdict:** Tree-sitter is the right choice for Nika. Already in use.

#### Recommended Extensions to Nika's Highlighter

1. **Undercurl for diagnostics:** ratatui 0.30 supports `Modifier::UNDERCURLED` (curly underline). crossterm 0.29 emits `CSI 4::4 m`. Supported by Kitty, WezTerm, iTerm2, Alacritty.

```rust
// In diagnostics.rs, upgrade underline_style():
DiagnosticSeverity::Error => Style::default()
    .fg(Color::Rgb(220, 50, 47))
    .add_modifier(Modifier::UNDERCURLED),  // was: UNDERLINED
```

2. **Colored underlines:** Use `Style::underline_color()` for separate underline color (keeps text color intact while showing red/orange/blue underlines).

3. **Inline diagnostic text:** Render diagnostic messages as dim text at end of line (like Helix/rust-analyzer virtual text), or in a floating panel below the cursor line.

### 3. Helix Editor Architecture Lessons

Helix is the gold standard for terminal editor LSP integration. Key patterns:

#### LSP Client Architecture

```
Editor State
    |
    v
helix-lsp (JSON-RPC over stdio)
    |
    +-- textDocument/didOpen, didChange, didSave
    +-- textDocument/completion -> Popup list
    +-- textDocument/hover -> Info panel
    +-- textDocument/publishDiagnostics -> Gutter icons + underlines
    +-- textDocument/inlayHint -> Virtual text overlays
```

- Helix uses an internal `helix-lsp` module for JSON-RPC over stdin/stdout
- Language servers are configured via `languages.toml` -- declarative, not hardcoded
- LSP responses are processed async and cached in editor state
- Diagnostics are rendered per-character with styled spans

#### What Nika Can Borrow

Nika already has `nika-lsp-core` and `tower-lsp-server 0.23`. The LSP server exists. For the TUI editor:

1. **Connect TUI as LSP client:** When editing a `.nika.yaml` file in Studio view, spawn `nika lsp` as a child process and communicate via stdio JSON-RPC. This gives completions, hover, and diagnostics for free.

2. **Alternative: Direct library integration.** Since `nika-lsp-core` is a library crate in the same workspace, call its analysis functions directly without the JSON-RPC overhead. The `DiagnosticsEngine` already does this for parse/analyze errors.

3. **Recommendation:** Extend `DiagnosticsEngine` with LSP-quality features (completions, hover) by calling `nika-lsp-core` directly, avoiding the serialization overhead of a full LSP client-server setup.

### 4. Text Buffer Data Structures

| Structure | Crate | Use Case | Performance |
|-----------|-------|----------|-------------|
| **Ropey** | [ropey](https://crates.io/crates/ropey) 2.0-beta | Large files, many edits | O(log N) insert/delete |
| **Gap Buffer** | custom | Small-medium files | O(1) at cursor, O(N) elsewhere |
| **Vec\<String\>** | std | Simple, small files | O(N) insert/delete |

- **Ropey:** 857,980 downloads/month. Used by Helix, Zee, Amp. The standard for Rust text editors.
- **Current Nika:** Uses `Vec<String>` (lines array) in TextBuffer. Fine for `.nika.yaml` files which are typically 50-500 lines.
- **Recommendation:** For YAML workflow files, `Vec<String>` is sufficient. Do NOT add ropey unless editing files > 10K lines becomes a use case. The complexity is not justified for workflow files.

### 5. Autocomplete/Dropdown Widgets

No mature, production-ready autocomplete dropdown exists for ratatui. Options:

#### ratatui-interact

- **Crate:** [ratatui-interact](https://crates.io/crates/ratatui-interact) | [GitHub](https://github.com/Brainwires/ratatui-interact)
- **Version:** 0.4.2 (2026-02-04)
- **Downloads:** 1,010 total (very new)
- **Features:** Select dropdown, ContextMenu, FocusManager
- **Verdict:** Too new and thin for production use. But good reference for API design.

#### Custom Implementation Pattern (Recommended)

Helix and similar editors build completion menus as floating overlays:

```rust
// 1. Track completion state
struct CompletionState {
    items: Vec<CompletionItem>,
    selected: usize,
    visible: bool,
    anchor: Position,  // Where to render the popup
}

// 2. Render as overlay after main editor
fn render_completion(f: &mut Frame, state: &CompletionState, editor_area: Rect) {
    if !state.visible { return; }

    // Position popup below cursor
    let popup_area = Rect::new(
        editor_area.x + state.anchor.col as u16,
        editor_area.y + state.anchor.line as u16 + 1,
        40,  // width
        state.items.len().min(10) as u16,  // max 10 visible
    );

    // Clear background and render bordered list
    f.render_widget(Clear, popup_area);
    let items: Vec<ListItem> = state.items.iter()
        .map(|i| ListItem::new(i.label.clone()))
        .collect();
    let list = List::new(items)
        .highlight_style(Style::default().reversed())
        .block(Block::bordered());
    f.render_stateful_widget(list, popup_area, &mut ListState::default());
}
```

Key techniques:
- Use `Clear` widget to erase background before drawing popup
- Compute popup `Rect` relative to cursor position
- Clamp to terminal bounds to avoid overflow
- Use `ListState` for keyboard navigation
- Filter items on keystroke using `nucleo` (already in Nika's deps)

### 6. YAML-Specific Editor Features

#### Schema Validation (Already in place)

Nika already has:
- `WorkflowSchemaValidator` for JSON Schema validation against `nika/workflow@0.12`
- `DiagnosticsEngine` bridging analyzer errors to TUI diagnostics
- Real-time validation on text change with debouncing

#### Recommended Enhancements

1. **Completion items from schema:**
   - At top level: suggest `nika:`, `name:`, `tasks:`
   - Inside task: suggest verb keys (`infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`)
   - Inside `infer:`: suggest `prompt:`, `model:`, `provider:`, `content:`, `output:`
   - Inside `with:`: suggest `$task_id` references from DAG

2. **Hover information:**
   - On verb keys: show verb documentation
   - On `model:` values: show provider and capabilities
   - On `$task_id` references: show task output schema
   - On `{{with.x}}` templates: show resolved value type

3. **Go-to-definition:**
   - On `depends_on: [task_id]`: jump to task definition
   - On `with: { alias: $task_id }`: jump to source task
   - On `invoke: server.tool`: show MCP tool schema

4. **Inline diagnostics panel:**
   - Bottom panel showing all diagnostics with severity icons
   - Click/Enter to jump to diagnostic location
   - Filter by severity (errors only, warnings, hints)

### 7. Real-Time Validation Display Patterns

Best patterns from the research:

#### Gutter Icons (Already implemented)
```
  1  ● name: bad-workflow     # Error icon in gutter
  2  ▲ tasks:                 # Warning icon in gutter
  3    - id: task1            # Clean line
```

#### Inline Virtual Text (Recommended addition)
```
  1  name: bad-workflow     ← NIKA-012: Invalid schema version
  2  tasks:                 ← NIKA-020: Circular dependency detected
  3    - id: task1
```

Render as dim/italic `Span` appended to the end of each line. Only show for the first diagnostic per line.

#### Diagnostic Panel (Bottom bar, already partially exists)
```
 ┌─ Diagnostics (2 errors, 1 warning) ────────────────────────────┐
 │ E  L1:6  NIKA-012  Invalid workflow schema version             │
 │ E  L2:3  NIKA-020  Circular dependency: task1 → task2 → task1  │
 │ W  L5:8  NIKA-041  Unused binding 'data'                       │
 └────────────────────────────────────────────────────────────────┘
```

#### Undercurl Underlines (New capability)
ratatui 0.30 + crossterm 0.29 support `Modifier::UNDERCURLED`:
- Red undercurl for errors
- Orange undercurl for warnings
- Blue undercurl for hints
- Terminal support: Kitty, WezTerm, iTerm2, Alacritty (all macOS-relevant)
- Fallback: straight underline on unsupported terminals

### 8. LSP Integration Architecture for TUI

Two viable architectures for connecting the Nika LSP to the Studio view:

#### Option A: In-Process (Recommended for Nika)

```
StudioView
    |
    v
nika-lsp-core (library calls)
    |
    +-- analyze_text() -> Diagnostics
    +-- get_completions(position) -> Vec<CompletionItem>
    +-- get_hover(position) -> HoverInfo
    +-- get_definitions(position) -> Vec<Location>
```

- No serialization overhead
- Direct access to AST and analysis results
- Already partially implemented via DiagnosticsEngine
- Extend nika-lsp-core with completion/hover APIs that return Rust types (not JSON)

#### Option B: Out-of-Process (Standard LSP)

```
StudioView (LSP Client)
    |  JSON-RPC over stdio
    v
nika lsp (child process)
    |
    v
nika-lsp-core
```

- Standard LSP protocol
- Same code path as VS Code/Helix users
- More complex but tests the real LSP implementation
- Use `rmcp` or a lightweight JSON-RPC client

**Recommendation:** Start with Option A (direct library calls) for speed and simplicity. The DiagnosticsEngine is already doing this for parse errors. Extend it to cover completions and hover. Consider Option B later if you want to dogfood the LSP server.

---

## Crate Comparison Matrix

| Crate | Version | Downloads | Last Update | Ratatui 0.30 | Nika Fit |
|-------|---------|-----------|-------------|-------------|----------|
| **edtui** | 0.11.2 | 92K | 2026-03-08 | Yes | Study, don't adopt |
| **ratatui-textarea** | 0.8.0 | 20K | 2026-02-21 | Yes | Best candidate for editing core |
| **tui-textarea** | 0.7.0 | 1.26M | 2024-10-22 | Partial | Legacy, avoid |
| **ratatui-code-editor** | 0.0.3 | 638 | 2026-02-28 | Yes | Pre-alpha, watch |
| **tui-syntax** | 0.5.0 | 400 | 2026-03-02 | Yes | Not needed (Nika has better) |
| **ratatui-interact** | 0.4.2 | 1K | 2026-02-04 | Yes | Too early for production |
| **ropey** | 2.0-beta | 858K/mo | Active | N/A | Not needed for YAML files |
| **tui-input** | 0.15 | (in use) | Active | Yes | Keep for chat input |

---

## Recommended Action Plan

### Phase 1: Quick Wins (1-2 days)

1. **Upgrade diagnostic underlines to undercurl:**
   - Change `Modifier::UNDERLINED` to `Modifier::UNDERCURLED` in `diagnostics.rs`
   - Add `Style::underline_color()` for colored underlines independent of text color
   - Graceful fallback on terminals without undercurl support

2. **Add inline virtual text diagnostics:**
   - Append dim diagnostic text at end of lines with errors
   - Truncate to fit available width

### Phase 2: Editor Core Upgrade (3-5 days)

3. **Evaluate ratatui-textarea 0.8.0 integration:**
   - Replace custom TextBuffer with `ratatui-textarea::TextArea`
   - Plug `TreeSitterHighlighter` into its custom highlighting hook
   - Preserve Nika-specific keybindings (Normal/Insert mode switching)
   - Verify undo/redo, selection, search work correctly

4. **OR: Enhance custom editor with borrowed patterns:**
   - Study edtui's text manipulation (word motions, visual line selection)
   - Study ratatui-textarea's soft wrap and search
   - Implement missing features in Nika's TextBuffer directly

### Phase 3: Intelligent Editing (1-2 weeks)

5. **Schema-driven completions:**
   - Build completion items from workflow schema
   - Render as floating `List` popup below cursor
   - Use `nucleo` for fuzzy filtering
   - Trigger on `:` after top-level keys and verb keys

6. **Hover information:**
   - On cursor hold (debounced), show info panel
   - Verb documentation, model capabilities, task output schemas
   - Render in a floating bordered panel

7. **Go-to-definition:**
   - `gd` in Normal mode on task references
   - Jump cursor to task definition line

### Phase 4: Full LSP Features (2-3 weeks)

8. **Extend nika-lsp-core with TUI-compatible APIs:**
   - `get_completions(text: &str, position: Position) -> Vec<CompletionItem>`
   - `get_hover(text: &str, position: Position) -> Option<HoverInfo>`
   - `get_definitions(text: &str, position: Position) -> Vec<Location>`

9. **Diagnostic panel widget:**
   - Scrollable list of all diagnostics
   - Click/keyboard to jump to source location
   - Filter by severity

---

## Sources

1. [edtui on crates.io](https://crates.io/crates/edtui) -- Vim-modal editor widget (0.11.2)
2. [edtui on GitHub](https://github.com/preiter93/edtui) -- Source and examples
3. [ratatui-textarea on GitHub](https://github.com/ratatui/ratatui-textarea) -- Official ratatui org fork (0.8.0)
4. [tui-textarea on GitHub](https://github.com/rhysd/tui-textarea) -- Original by rhysd (legacy)
5. [ratatui-code-editor on GitHub](https://github.com/vipmax/ratatui-code-editor) -- Tree-sitter editor (0.0.3)
6. [tui-syntax on crates.io](https://crates.io/crates/tui-syntax) -- Tree-sitter highlighting (0.5.0)
7. [ropey on lib.rs](https://lib.rs/crates/ropey) -- Rope data structure (2.0-beta)
8. [ratatui-interact on GitHub](https://github.com/Brainwires/ratatui-interact) -- Select/dropdown widgets (0.4.2)
9. [Helix editor](https://github.com/helix-editor/helix) -- LSP integration reference
10. [awesome-ratatui](https://github.com/ratatui/awesome-ratatui) -- Widget ecosystem catalog
11. [ratatui docs](https://docs.rs/ratatui/0.30) -- Modifier::UNDERCURLED, Style::underline_color()
12. [Helix languages.toml](https://docs.helix-editor.com/lang-support.html) -- Declarative LSP config
13. [json-schema-everywhere YAML](https://json-schema-everywhere.github.io/yaml) -- YAML schema validation patterns
14. [Ryan Travitz: Pull of the Undercurl](https://ryantravitz.com/blog/2023-02-18-pull-of-the-undercurl/) -- Terminal undercurl escape codes

## Methodology

- Tools used: Perplexity search (9 queries), crates.io API (7 crate lookups), source code review (5 Nika files)
- Pages analyzed: 14 primary sources + 40+ secondary citations
- Ecosystem coverage: All ratatui editor widgets on awesome-ratatui, all major Rust terminal editors

## Confidence Level

**High** -- All crate versions and download counts verified against crates.io API on 2026-03-21. Architecture recommendations based on direct analysis of Nika's existing codebase. Helix patterns well-documented in public sources.

## Key Takeaway

Nika is already ahead of most ratatui projects with its custom tree-sitter highlighter, Nika-specific captures, DiagnosticsEngine, EditHistory, and SelectionSet. The gap is in the editing core (TextBuffer vs ratatui-textarea) and intelligent features (completions, hover, go-to-definition). The recommended path is to evaluate ratatui-textarea 0.8.0 as the editing foundation, keep the custom tree-sitter highlighter, and extend DiagnosticsEngine into a full in-process LSP client by leveraging nika-lsp-core directly.
