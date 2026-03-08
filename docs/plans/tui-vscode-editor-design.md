# Design: VS Code-like Editor for Nika TUI

**Status**: Draft
**Date**: 2026-03-08
**Author**: Claude + Thibaut

## Current State

Le Studio view actuel (`src/tui/views/studio.rs`) a déjà une base solide:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ BROWSER (20%)      │ EDITOR (50%)                  │ STRUCTURE (30%)       │
│ TreeWidget         │ YamlEditorPanel               │ DAG mini-view         │
│ ├── Quick Access   │ ├── Line numbers              │ Task dependencies     │
│ └── File tree      │ ├── Syntax highlighting       │                       │
│                    │ └── Validation badge          │                       │
├────────────────────┴──────────────────────────────┴───────────────────────┤
│ Valid YAML │ Schema OK │ 1 warning                                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Fonctionnalités existantes

| Feature | Status | Notes |
|---------|--------|-------|
| 3-panel layout | ✅ | Browser/Editor/DAG avec ratios dynamiques |
| File browser | ✅ | TreeWidget avec git status, filtres (W/A/E/0) |
| Quick Access | ✅ | 5 fichiers .nika.yaml récents |
| YAML highlighting | ✅ | Coloration syntaxique basique |
| Line numbers | ✅ | Affichés à gauche |
| Vim modes | ✅ | Normal/Insert |
| Undo/Redo | ✅ | EditHistory (Ctrl+Z/Y) |
| Validation | ✅ | YAML + Schema |
| Matrix rain | ✅ | Effet visuel optionnel |

### Limitations actuelles

1. **TextBuffer basique** - Pas de sélection, pas de multi-curseur
2. **Navigation limitée** - Pas de go-to-definition, pas de breadcrumbs
3. **Pas d'auto-completion** - Crucial pour YAML workflows
4. **Pas de minimap** - VS Code signature
5. **Pas de split view** - Edition de plusieurs fichiers
6. **Pas de search/replace** - Recherche dans fichier
7. **Pas de problems panel** - Liste des erreurs navigable

---

## Design Proposé: Studio Pro v0.23

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ ◀ ◆ ▶                  │ workflow.nika.yaml ●      │                        │
│ Explorer │ Search │ Git │ ← Tabs multiples         │ [1] [2] [3] Layout    │
├───────────────────────────────────────────────────────────────────────────────┤
│ EXPLORER            │ EDITOR                           │ STRUCTURE          │
│ ┌─ QUICK ACCESS ──┐ │ ┌─ Breadcrumbs ─────────────────┐ │ ┌─ DAG ─────────┐ │
│ │ ⚡ recent-1.nika│ │ │ workflow > tasks > generate    │ │ │               │ │
│ │ ⚡ recent-2.nika│ │ └────────────────────────────────┘ │ │  ┌─────────┐  │ │
│ └─────────────────┘ │                                    │ │  │ infer   │  │ │
│ ┌─ WORKSPACE ─────┐ │  1 │ workflow:                     │ │  │ (LLM)   │  │ │
│ │ 📁 workflows    │ │  2 │   name: generate-content      │ │  └────┬────┘  │ │
│ │   📄 main.nika  │ │  3 │                               │ │       ▼       │ │
│ │   📄 seo.nika   │ │  4 │ tasks:                        │ │  ┌─────────┐  │ │
│ │ 📁 includes     │ │  5 │   - id: fetch_context         │ │  │ exec    │  │ │
│ │   📄 shared.yaml│ │  6 │     invoke: novanet_generate  │ │  │ (bash)  │  │ │
│ └─────────────────┘ │  7 │     params:                   │ │  └─────────┘  │ │
│ ┌─ OUTLINE ───────┐ │  8 │       focus_key: $entity      │ │               │ │
│ │ ◇ workflow      │ │  9 │       locale: fr-FR           │ │ ┌───────────┐ │ │
│ │   ◆ tasks       │ │ 10 │                               │ │ │ MINIMAP   │ │ │
│ │     ○ fetch     │ │ 11 │   - id: generate_block        │ │ │           │ │ │
│ │     ○ generate  │ │ 12 │     infer: |                  │ │ │           │ │ │
│ │   ◆ hooks       │ │ 13 │       Generate landing page   │ │ │           │ │ │
│ └─────────────────┘ │    │       content for $entity     │ │ └───────────┘ │ │
├─────────────────────┴────┴───────────────────────────────┴──────────────────┤
│ PROBLEMS (2)       │ OUTPUT    │ DEBUG                                      │
│ ⚠ line 12: indent  │           │                                           │
│ ❌ line 8: missing │           │                                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Nouvelles Fonctionnalités

#### Phase 1: Editor Core (v0.23)

| Feature | Priority | Implementation |
|---------|----------|----------------|
| **ratatui-textarea** | P0 | Remplacer TextBuffer par [ratatui-textarea](https://github.com/ratatui/ratatui-textarea) |
| **Selection** | P0 | Shift+Arrow, Ctrl+Shift+End, etc. |
| **Copy/Paste** | P0 | Ctrl+C/X/V avec clipboard system |
| **Search in file** | P1 | Ctrl+F, regex support |
| **Go to line** | P1 | Ctrl+G |
| **Breadcrumbs** | P1 | YAML path: `workflow > tasks > id` |

#### Phase 2: Intelligence (v0.24)

| Feature | Priority | Implementation |
|---------|----------|----------------|
| **Auto-completion** | P0 | LSP-like pour Nika DSL (verbs, params) |
| **Semantic highlighting** | P0 | Verbs colorés (infer: bleu, exec: vert) |
| **Problems panel** | P1 | Liste navigable des erreurs/warnings |
| **Go to definition** | P1 | F12 sur `$variable` ou `include:` |
| **Hover info** | P2 | Tooltip sur hover |

#### Phase 3: Navigation (v0.25)

| Feature | Priority | Implementation |
|---------|----------|----------------|
| **Tab bar** | P0 | Multiple fichiers ouverts |
| **Split view** | P1 | Vertical/horizontal split |
| **Minimap** | P2 | Vue condensée à droite |
| **Outline view** | P1 | Structure YAML navigable |
| **Symbol search** | P2 | Ctrl+Shift+O pour tasks/hooks |

### Keybindings (VS Code Compatible)

```
Navigation:
  Ctrl+G        Go to line
  Ctrl+P        Quick open file
  Ctrl+Shift+O  Go to symbol
  F12           Go to definition
  Ctrl+Tab      Switch between tabs

Editing:
  Ctrl+C/X/V    Copy/Cut/Paste
  Ctrl+Z/Y      Undo/Redo
  Ctrl+D        Select next occurrence
  Ctrl+/        Toggle comment
  Alt+Up/Down   Move line up/down

Search:
  Ctrl+F        Find in file
  Ctrl+H        Find and replace
  Ctrl+Shift+F  Find in workspace

View:
  Ctrl+B        Toggle sidebar
  Ctrl+`        Toggle terminal/output
  Ctrl+Shift+E  Focus explorer
```

### Implementation Notes

#### ratatui-textarea Integration

```rust
// Current: TextBuffer (basic)
pub struct YamlEditorPanel {
    pub buffer: TextBuffer,
    // ...
}

// Proposed: ratatui-textarea
use ratatui_textarea::TextArea;

pub struct YamlEditorPanel {
    pub editor: TextArea<'static>,
    pub highlighter: YamlHighlighter,
    pub completer: NikaCompleter,
    // ...
}
```

#### Auto-completion Schema

```rust
/// Nika DSL completion provider
pub struct NikaCompleter {
    /// Known verbs: infer, exec, fetch, invoke, agent
    verbs: Vec<CompletionItem>,
    /// Known MCP tools from NovaNet
    mcp_tools: Vec<CompletionItem>,
    /// Workflow-local variables
    variables: HashMap<String, CompletionItem>,
}

impl NikaCompleter {
    /// Trigger completion on: `$`, `:`, `invoke:`, etc.
    pub fn complete(&self, context: &CompletionContext) -> Vec<CompletionItem> {
        match context.trigger {
            '$' => self.variables.values().cloned().collect(),
            ':' => self.verbs.clone(),
            _ => vec![],
        }
    }
}
```

#### Problems Panel

```rust
/// Diagnostic entry in problems panel
pub struct Diagnostic {
    pub severity: DiagnosticSeverity, // Error, Warning, Info
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub source: &'static str, // "yaml", "nika", "schema"
}

/// Problems panel state
pub struct ProblemsPanel {
    pub diagnostics: Vec<Diagnostic>,
    pub selected: usize,
    pub filter: DiagnosticSeverity,
}
```

### Migration Path

```
v0.22 (current)
  │
  ├── Phase 1: Editor Core (v0.23)
  │     └── Add ratatui-textarea dependency
  │     └── Migrate TextBuffer → TextArea
  │     └── Add selection, search, breadcrumbs
  │
  ├── Phase 2: Intelligence (v0.24)
  │     └── NikaCompleter with verb/param completion
  │     └── Problems panel with navigation
  │     └── Semantic highlighting (verb colors)
  │
  └── Phase 3: Navigation (v0.25)
        └── Tab bar for multiple files
        └── Split view
        └── Outline panel
        └── Minimap
```

### Performance Considerations

1. **Lazy validation** - Debounce validation on keystroke (300ms)
2. **Incremental parsing** - tree-sitter for YAML parsing
3. **Virtual scrolling** - Only render visible lines (TextArea handles this)
4. **Cached completions** - Refresh MCP tools on demand, not every keystroke

### Testing Strategy

1. **Unit tests** - Completer, highlighter, diagnostics
2. **Snapshot tests** - UI rendering with insta
3. **Integration tests** - Full editor workflow (open, edit, validate, save)
4. **Property tests** - Cursor movement invariants

---

## Decision Record

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-03-08 | Use ratatui-textarea | Official ratatui org fork, maintained, ratatui 0.30 compatible |
| 2026-03-08 | 3-phase rollout | Incremental value delivery, lower risk |
| 2026-03-08 | VS Code keybindings | Familiar to users, industry standard |

## References

- [ratatui-textarea](https://github.com/ratatui/ratatui-textarea) - Editor widget
- [tree-sitter](https://tree-sitter.github.io/) - Incremental parsing
- [VS Code Keybindings](https://code.visualstudio.com/docs/getstarted/keybindings) - Reference
