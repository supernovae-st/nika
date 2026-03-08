# World-Class Terminal IDE Design for Nika Studio

> **Vision**: Nika Studio = Helix + VS Code + Zed for YAML Workflows

**Date**: 2026-03-08
**Version Target**: v0.23 - v0.26
**Status**: Design Complete

---

## Executive Summary

Transform Nika's Studio view into a world-class terminal IDE with:

- **Tree-sitter YAML highlighting** for semantic, structural awareness
- **LSP integration** with yaml-language-server + custom nika-lsp
- **Command palette** with fuzzy search (nucleo)
- **Which-key keybinding discovery**
- **Enhanced tree view** with bookmarks, fuzzy search, workspace folders

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA STUDIO v0.23+ ARCHITECTURE                                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │                           COMMAND PALETTE (⌘K)                          │   │
│  │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│  │  │  > format                                                    ⇧⌥F │    │   │
│  │  │  ❯ Format Document                               yaml-language │    │   │
│  │  │    Format Selection                              yaml-language │    │   │
│  │  │    Run Workflow                                          ⌘⏎    │    │   │
│  │  └─────────────────────────────────────────────────────────────────┘    │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│  ┌────────────────┬───────────────────────────────┬────────────────────────┐   │
│  │   TREE VIEW    │          EDITOR               │      DAG PREVIEW       │   │
│  │    (20%)       │          (50%)                │        (30%)           │   │
│  │                │                               │                        │   │
│  │  📁 workflows  │  1│ schema: nika/workflow@0.9 │   ┌─────┐              │   │
│  │  ├── 📄 gen.   │  2│                           │   │fetch│              │   │
│  │  │    ⭐ M     │  3│ tasks:                    │   └──┬──┘              │   │
│  │  ├── 📄 build  │  4│   - id: fetch_data       │      │                  │   │
│  │  └── 📁 tests  │  5│     fetch:                │   ┌──▼──┐              │   │
│  │      ├── 📄 u. │  6│       url: https://...    │   │infer│              │   │
│  │      └── 📄 i. │  7│   ~~~~~~~~~~~~            │   └─────┘              │   │
│  │                │     ^ squiggle error          │                        │   │
│  │  ───────────── │                               │  [Tasks: 2]            │   │
│  │  🔍 Search...  │  K → hover doc               │  [Deps: 1]             │   │
│  │                │  gd → go to def               │  [Errors: 1]           │   │
│  └────────────────┴───────────────────────────────┴────────────────────────┘   │
│                                                                                 │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │  DIAGNOSTICS (F8)                                                       │   │
│  │  ⚠️ Line 7: Unknown property "urll" - did you mean "url"?               │   │
│  │  ❌ Line 12: Task "missing_task" not found in workflow                  │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Syntax Highlighting: Tree-sitter

### Why Tree-sitter?

| Approach | Pros | Cons |
|----------|------|------|
| **Tree-sitter** | Incremental parsing, structural editing, semantic awareness | ~500KB binary size |
| Syntect | Mature, Sublime grammars | Line-by-line parsing |
| Custom regex | Zero deps | Maintenance burden |

**Decision**: Tree-sitter (like Helix editor)

### Implementation

```
src/tui/highlight/
├── mod.rs              # Highlighter trait
├── treesitter.rs       # Tree-sitter implementation
├── queries/
│   └── yaml.scm        # Highlight queries
└── themes/
    ├── solarized.rs    # Color mappings
    └── mod.rs          # Theme trait
```

### Highlight Query Example (yaml.scm)

```scheme
; Keys
(block_mapping_pair
  key: (flow_node) @keyword)

; Strings
(double_quote_scalar) @string
(single_quote_scalar) @string

; Numbers
(integer_scalar) @number
(float_scalar) @number

; Booleans
((plain_scalar) @constant.builtin
  (#match? @constant.builtin "^(true|false)$"))

; Comments
(comment) @comment

; Nika-specific: task verbs
((plain_scalar) @function
  (#match? @function "^(infer|exec|fetch|invoke|agent)$"))

; Nika-specific: task IDs
(block_mapping_pair
  key: (flow_node
    (plain_scalar) @property
    (#eq? @property "id"))
  value: (flow_node) @label)
```

### Dependencies

```toml
[dependencies]
tree-sitter = "0.24"
tree-sitter-yaml = "0.6"
```

---

## 2. LSP Integration

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Editor (Nika TUI)                        │
│                              │                                  │
│                        LspClient                                │
│                    (JSON-RPC over stdio)                        │
│                              │                                  │
│              ┌───────────────┴───────────────┐                  │
│              │                               │                  │
│              ▼                               ▼                  │
│   yaml-language-server              nika-lsp (future)           │
│   (RedHat, npm package)             (Rust, built-in)            │
│                                                                 │
│   Features:                         Features:                   │
│   ├── JSON Schema validation        ├── Task ID completions     │
│   ├── Key/value completions         ├── MCP tool completions    │
│   ├── Hover documentation           ├── Go-to-definition        │
│   └── Formatting                    └── Workflow diagnostics    │
└─────────────────────────────────────────────────────────────────┘
```

### LspClient Module

```rust
// src/tui/lsp/client.rs

pub struct LspClient {
    process: Child,
    request_id: AtomicU64,
    pending: DashMap<u64, oneshot::Sender<Response>>,
    diagnostics_tx: broadcast::Sender<Vec<Diagnostic>>,
}

impl LspClient {
    /// Spawn yaml-language-server process
    pub async fn spawn_yaml_lsp() -> Result<Self> {
        let process = Command::new("yaml-language-server")
            .args(["--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        // ...
    }

    /// Send textDocument/didOpen notification
    pub async fn did_open(&self, uri: &str, content: &str) -> Result<()>;

    /// Request textDocument/completion
    pub async fn completion(&self, uri: &str, position: Position) -> Result<Vec<CompletionItem>>;

    /// Request textDocument/hover
    pub async fn hover(&self, uri: &str, position: Position) -> Result<Option<Hover>>;
}
```

### Diagnostics Overlay

```rust
// src/tui/widgets/diagnostics.rs

pub struct DiagnosticsOverlay {
    diagnostics: Vec<Diagnostic>,
    selected: usize,
}

impl DiagnosticsOverlay {
    /// Render inline squiggles in editor
    pub fn render_inline(&self, line: usize, buf: &mut Buffer, theme: &Theme) {
        for diag in self.diagnostics_for_line(line) {
            let style = match diag.severity {
                DiagnosticSeverity::Error => theme.error_underline(),
                DiagnosticSeverity::Warning => theme.warning_underline(),
                _ => theme.info_underline(),
            };
            // Draw squiggle under affected range
            for col in diag.range.start.character..diag.range.end.character {
                buf.set_style(Rect::new(col, line, 1, 1), style);
            }
        }
    }

    /// Render gutter icons
    pub fn render_gutter(&self, line: usize) -> Option<Span> {
        self.diagnostics_for_line(line)
            .max_by_key(|d| d.severity)
            .map(|d| match d.severity {
                DiagnosticSeverity::Error => Span::styled("●", Style::default().fg(Color::Red)),
                DiagnosticSeverity::Warning => Span::styled("●", Style::default().fg(Color::Yellow)),
                _ => Span::styled("●", Style::default().fg(Color::Blue)),
            })
    }
}
```

---

## 3. Command Palette

### Fuzzy Search with Nucleo

```toml
[dependencies]
nucleo = "0.5"  # Same as Helix uses
```

### Command Registry

```rust
// src/tui/palette/commands.rs

pub struct Command {
    pub id: &'static str,
    pub label: &'static str,
    pub keybinding: Option<&'static str>,
    pub source: CommandSource,
    pub action: CommandAction,
}

pub enum CommandSource {
    Editor,
    Lsp(&'static str),  // "yaml-language-server"
    Nika,
}

pub static COMMANDS: &[Command] = &[
    Command {
        id: "editor.formatDocument",
        label: "Format Document",
        keybinding: Some("⇧⌥F"),
        source: CommandSource::Lsp("yaml-language-server"),
        action: CommandAction::Format,
    },
    Command {
        id: "nika.runWorkflow",
        label: "Run Workflow",
        keybinding: Some("⌘⏎"),
        source: CommandSource::Nika,
        action: CommandAction::RunWorkflow,
    },
    // ...
];
```

### Which-Key Widget

```rust
// src/tui/widgets/which_key.rs

pub struct WhichKeyPopup {
    leader: char,
    bindings: Vec<(char, &'static str)>,
    timeout: Duration,
}

impl WhichKeyPopup {
    pub fn for_leader(leader: char) -> Self {
        let bindings = match leader {
            'g' => vec![
                ('d', "Go to definition"),
                ('r', "Find references"),
                ('i', "Go to implementation"),
                ('h', "Show hover"),
            ],
            'z' => vec![
                ('z', "Center cursor"),
                ('t', "Cursor to top"),
                ('b', "Cursor to bottom"),
            ],
            _ => vec![],
        };
        Self {
            leader,
            bindings,
            timeout: Duration::from_millis(500),
        }
    }
}
```

---

## 4. Enhanced Tree View

### New Features

| Feature | Hotkey | Description |
|---------|--------|-------------|
| Fuzzy search | Ctrl+P (in browser) | Filter tree by filename |
| Bookmark | M | Pin file to quick access |
| Workspace folders | - | Multi-root support |
| File icons | - | Nerd Font icons by extension |

### Implementation

```rust
// src/tui/widgets/tree/search.rs

pub struct TreeSearch {
    query: String,
    matcher: nucleo::Matcher,
    matches: Vec<TreeNodeRef>,
}

impl TreeSearch {
    pub fn filter(&mut self, root: &TreeNode) {
        self.matches = root
            .flatten()
            .filter(|node| {
                let score = self.matcher.fuzzy_match(&node.name, &self.query);
                score.is_some()
            })
            .collect();
    }
}
```

```rust
// src/tui/widgets/tree/bookmark.rs

pub struct BookmarkManager {
    bookmarks: HashSet<PathBuf>,
    recent: VecDeque<PathBuf>,  // LRU
}

impl BookmarkManager {
    pub fn toggle(&mut self, path: &Path) {
        if self.bookmarks.contains(path) {
            self.bookmarks.remove(path);
        } else {
            self.bookmarks.insert(path.to_path_buf());
        }
    }

    pub fn is_bookmarked(&self, path: &Path) -> bool {
        self.bookmarks.contains(path)
    }
}
```

---

## 5. Implementation Roadmap

### Phase 1: Foundation (v0.23)

- [ ] Tree-sitter YAML integration
  - [ ] Add dependencies (tree-sitter, tree-sitter-yaml)
  - [ ] Create Highlighter trait
  - [ ] Write YAML highlight queries
  - [ ] Integrate with editor rendering
- [ ] Enhanced command palette
  - [ ] Add nucleo fuzzy matching
  - [ ] Command registry
  - [ ] Which-key popup

### Phase 2: LSP Integration (v0.24)

- [ ] yaml-language-server client
  - [ ] LspClient with JSON-RPC
  - [ ] Diagnostics overlay
  - [ ] Completion popup
  - [ ] Hover documentation
- [ ] Problems panel
  - [ ] F8 cycle through errors
  - [ ] Click to jump

### Phase 3: Advanced (v0.25)

- [ ] Custom nika-lsp
  - [ ] Task ID completions
  - [ ] MCP tool completions
  - [ ] Go-to-definition
- [ ] Tree enhancements
  - [ ] Fuzzy file search
  - [ ] Bookmarks
  - [ ] Workspace folders

### Phase 4: Polish (v0.26)

- [ ] Structural editing (tree-sitter)
  - [ ] Select task block
  - [ ] Expand/shrink selection
- [ ] Multi-cursor editing
- [ ] Split editor
- [ ] Diff view

---

## 6. Dependencies

```toml
# Cargo.toml additions

[dependencies]
# Syntax highlighting
tree-sitter = "0.24"
tree-sitter-yaml = "0.6"

# Fuzzy matching (same as Helix)
nucleo = "0.5"

# LSP (optional, for yaml-language-server)
lsp-types = "0.95"
```

---

## 7. Research Sources

### Terminal IDE Inspirations

| Editor | Key Innovation | Adopted |
|--------|---------------|---------|
| **Helix** | Selection-first, built-in LSP, tree-sitter | ✅ Tree-sitter, LSP |
| **VS Code** | Command palette, 3-panel, git integration | ✅ Command palette, tree |
| **Zed** | GPU-accelerated, minimal latency | Partial (async) |
| **Kakoune** | Multi-cursor, structural selection | Future (v0.26) |
| **Neovim** | Extensibility, treesitter integration | ✅ Tree-sitter |

### Documentation

- [Helix LSP Implementation](https://github.com/helix-editor/helix/tree/master/helix-lsp)
- [yaml-language-server](https://github.com/redhat-developer/yaml-language-server)
- [tree-sitter-yaml](https://github.com/tree-sitter/tree-sitter-yaml)
- [Ratatui Editor Patterns](https://ratatui.rs/examples/user-input/)

---

## 8. Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Keystrokes to format | N/A | 2 (⇧⌥F) |
| Error discovery time | Manual | Real-time |
| File navigation | Arrow keys | Fuzzy search |
| Task definition lookup | grep | gd (instant) |
| Syntax highlighting | None | Semantic |

---

**Next Steps**:
1. Approve this design
2. Create git worktree for implementation
3. Start Phase 1 (tree-sitter integration)
