# Explorer View UX Proposal

> **For Claude:** Use this document when implementing Explorer view in v0.10.0

**Status:** Draft
**Date:** 2026-02-25
**Research Sources:** Perplexity Sonar, Context7 (ratatui), broot/yazi/ranger analysis

---

## Executive Summary

Based on comprehensive research of TUI file managers (broot, yazi, ranger, nvim-tree, neo-tree) and VS Code patterns, this proposal defines the UX for Nika's Explorer view with special focus on `.nika/` folder visualization.

---

## 1. `.nika/` Folder Structure

### Recommended Layout

```
.nika/
├── workflows/           # ⚡ Workflow YAML files
│   ├── generate.nika.yaml
│   └── deploy.nika.yaml
├── agents/              # 🐔 Agent configurations
│   └── orchestrator.agent.yaml
├── skills/              # 🔧 Custom skills
│   └── summarize.skill.yaml
├── context/             # 📦 Persistent context
│   ├── project.context.yaml
│   └── user.context.yaml
├── templates/           # 📋 Reusable templates
│   └── landing-page.template.yaml
├── traces/              # 📊 Execution traces (NDJSON)
│   └── 2026-02-25T10-30-00.ndjson
├── cache/               # 💾 Runtime cache
│   ├── mcp/
│   └── providers/
├── sessions/            # 🗂️ Editor sessions (v0.8.0)
│   └── current.json
└── config.toml          # ⚙️ User configuration
```

### Icon Mapping Table

| Folder/File | Icon | Color | Description |
|-------------|------|-------|-------------|
| `.nika/` | 🏠 | Emerald #10b981 | Project root marker |
| `workflows/` | ⚡ | Violet #8b5cf6 | DAG workflow definitions |
| `agents/` | 🐔 | Rose #f43f5e | Agent configurations |
| `skills/` | 🔧 | Amber #f59e0b | Custom skills |
| `context/` | 📦 | Cyan #06b6d4 | Persistent context files |
| `templates/` | 📋 | Sky #0ea5e9 | Reusable templates |
| `traces/` | 📊 | Slate #64748b | Execution history |
| `cache/` | 💾 | Stone #78716c | Dim (secondary) |
| `sessions/` | 🗂️ | Stone #78716c | Editor state |
| `config.toml` | ⚙️ | Zinc #71717a | Configuration |
| `*.nika.yaml` | 📄⚡ | Violet #8b5cf6 | Workflow file |
| `*.agent.yaml` | 📄🐔 | Rose #f43f5e | Agent file |

---

## 2. Keyboard Navigation

### Universal vim-style (from broot/yazi/ranger)

| Key | Action | Source |
|-----|--------|--------|
| `j` / `↓` | Move down | Universal |
| `k` / `↑` | Move up | Universal |
| `h` / `←` | Go to parent | yazi, ranger |
| `l` / `→` / `Enter` | Open/expand | yazi, ranger |
| `gg` | Go to first | vim |
| `G` | Go to last | vim |
| `Ctrl+u` | Page up | vim |
| `Ctrl+d` | Page down | vim |

### File Operations

| Key | Action | Source |
|-----|--------|--------|
| `.` | Toggle hidden files | yazi |
| `/` | Start fuzzy search | Universal |
| `Esc` | Cancel/clear search | Universal |
| `Space` | Toggle selection | ranger |
| `o` | Open in external editor | ranger |
| `y` | Copy path to clipboard | yazi |
| `r` | Rename | nvim-tree |
| `d` | Delete (with confirm) | nvim-tree |
| `a` | Create new file | nvim-tree |
| `A` | Create new folder | nvim-tree |

### Preview Controls

| Key | Action | Description |
|-----|--------|-------------|
| `Tab` | Toggle DAG/YAML preview | Switch preview mode |
| `p` | Toggle preview panel | Show/hide preview |
| `z` | Toggle zoom preview | Full-width preview |

### Quick Navigation

| Key | Action | Description |
|-----|--------|-------------|
| `~` | Go to `.nika/` | Jump to project config |
| `w` | Go to `workflows/` | Jump to workflows |
| `t` | Go to `traces/` | Jump to traces |
| `B` | Toggle bookmarks | Show bookmarked files |

---

## 3. Visual Layout

### Three-Panel Design (Miller Columns Inspired)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 🏠 qrcode-ai/.nika                                        [Explorer] 1/6   │
├──────────────────┬──────────────────────┬───────────────────────────────────┤
│ 📁 TREE (25%)    │ 🔷 DAG PREVIEW (35%) │ 📝 YAML PREVIEW (40%)             │
├──────────────────┼──────────────────────┼───────────────────────────────────┤
│                  │                      │                                   │
│ ▼ 🏠 .nika/      │    ┌─────────┐       │ ```yaml                           │
│   ▼ ⚡ workflows/ │    │ step1   │       │ schema: nika/workflow@0.5         │
│     ► generate   │    └────┬────┘       │ workflow: generate                │
│     ► deploy     │         │            │                                   │
│   ► 🐔 agents/   │    ┌────▼────┐       │ tasks:                            │
│   ► 🔧 skills/   │    │ step2   │       │   - id: step1                     │
│   ► 📦 context/  │    └────┬────┘       │     infer: "Generate..."          │
│ ▼ 📁 src/        │         │            │                                   │
│   ► components/  │    ┌────▼────┐       │   - id: step2                     │
│   ► pages/       │    │ step3   │       │     exec: "npm build"             │
│                  │    └─────────┘       │ ```                               │
│                  │                      │                                   │
├──────────────────┴──────────────────────┴───────────────────────────────────┤
│ [j/k] navigate  [l] open  [h] parent  [/] search  [.] hidden  [Tab] preview │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Breadcrumb Header

```
🏠 qrcode-ai / .nika / workflows / generate.nika.yaml
     ^           ^        ^            ^
     │           │        │            └─ Current file (bold)
     │           │        └─ Parent folder (clickable)
     │           └─ .nika root (🏠 icon)
     └─ Project root (dimmed)
```

### File List Entry Format

```
▼ ⚡ workflows/                  # Expanded folder
  │ ▸ generate.nika.yaml   3 tasks  # File with task count
  │ ▸ deploy.nika.yaml     5 tasks
► 🐔 agents/                     # Collapsed folder
► 🔧 skills/            (empty)  # Empty indicator
```

---

## 4. Interaction Patterns

### 4.1 Inline Fuzzy Search (broot-style)

```
┌─ Search: gen█ ───────────────────────────────────────┐
│                                                      │
│ ⚡ workflows/generate.nika.yaml          [1]        │
│ ⚡ workflows/generate-page.nika.yaml     [2]        │
│ 🐔 agents/content-generator.agent.yaml   [3]        │
│                                                      │
│ [Enter] open  [Tab] preview  [Esc] cancel            │
└──────────────────────────────────────────────────────┘
```

**Behavior:**
- Starts filtering immediately as you type
- Highlights matching characters
- Number shortcuts `[1-9]` for quick selection
- `Enter` opens, `Tab` previews without opening

### 4.2 Context Menu (on `m` or right-click)

```
┌─ generate.nika.yaml ─────────────────┐
│ ▸ Run workflow          [Enter]      │
│ ▸ Open in Studio        [s]          │
│ ▸ View traces           [t]          │
│ ─────────────────────────────────────│
│ ▸ Copy path             [y]          │
│ ▸ Rename                [r]          │
│ ▸ Delete                [d]          │
│ ─────────────────────────────────────│
│ ▸ New workflow here     [a]          │
└──────────────────────────────────────┘
```

### 4.3 Status Indicators

| Indicator | Meaning |
|-----------|---------|
| `●` | Modified (unsaved changes) |
| `✓` | Valid workflow |
| `✗` | Invalid YAML/schema |
| `◐` | Running execution |
| `⏸` | Paused execution |

---

## 5. Implementation Priorities

### P0: Core Navigation (v0.10.0)

1. **vim-style navigation** (j/k/h/l)
2. **Tree expansion/collapse** with state persistence
3. **`.nika/` folder detection and special styling**
4. **Breadcrumb header** with clickable segments
5. **Icon mapping** for known file types

### P1: Search & Preview (v0.10.1)

1. **Inline fuzzy search** (`/` key)
2. **DAG preview panel** (35% width)
3. **YAML preview panel** with syntax highlighting
4. **Preview toggle** (`Tab` key)
5. **Hidden files toggle** (`.` key)

### P2: Advanced Features (v0.10.2+)

1. **File operations** (create, rename, delete)
2. **Bookmarks system**
3. **Context menu**
4. **Multi-select** with `Space`
5. **External editor integration**

---

## 6. ratatui Implementation Notes

### Tree Widget Structure

```rust
use ratatui::widgets::{List, ListItem, ListState};

pub struct ExplorerTree {
    items: Vec<TreeNode>,
    state: ListState,
    expanded: HashSet<PathBuf>,  // Track expanded folders
}

pub struct TreeNode {
    path: PathBuf,
    depth: usize,
    is_dir: bool,
    icon: &'static str,
    color: Color,
}

impl ExplorerTree {
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem> = self.visible_items()
            .map(|node| {
                let indent = "  ".repeat(node.depth);
                let prefix = if node.is_dir {
                    if self.expanded.contains(&node.path) { "▼" } else { "►" }
                } else { " " };
                ListItem::new(format!("{}{} {} {}",
                    indent, prefix, node.icon, node.name()))
                    .style(Style::default().fg(node.color))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol("│ ");

        StatefulWidget::render(list, area, buf, &mut self.state);
    }
}
```

### Keyboard Handling

```rust
fn handle_key(&mut self, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => self.move_down(),
        KeyCode::Char('k') | KeyCode::Up => self.move_up(),
        KeyCode::Char('h') | KeyCode::Left => self.go_parent(),
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.open_or_expand(),
        KeyCode::Char('.') => self.toggle_hidden(),
        KeyCode::Char('/') => self.start_search(),
        KeyCode::Char('~') => self.go_to_nika(),
        KeyCode::Tab => self.toggle_preview_mode(),
        _ => Action::None,
    }
}
```

---

## 7. Comparison with Current Implementation

| Feature | v0.8.x (Home) | v0.10.0 (Explorer) |
|---------|---------------|---------------------|
| Layout | Flat file list | Nested tree |
| Navigation | Arrow keys only | vim-style (j/k/h/l) |
| .nika/ visibility | Indicator only | Full tree expansion |
| Folders | Not shown | Collapsible with icons |
| Search | None | Inline fuzzy (`/`) |
| Preview | DAG only | DAG + YAML toggle |
| Hidden files | Always hidden | Toggle with `.` |
| Breadcrumbs | None | Full path in header |

---

## 8. References

- **broot**: https://github.com/Canop/broot - Inline fuzzy search pattern
- **yazi**: https://github.com/sxyazi/yazi - Miller columns, `.` toggle
- **ranger**: https://github.com/ranger/ranger - Three-column layout
- **nvim-tree**: https://github.com/nvim-tree/nvim-tree.lua - File operations
- **neo-tree**: https://github.com/nvim-neo-tree/neo-tree.nvim - Git integration
- **VS Code**: Explorer sidebar patterns, `.vscode/` conventions
- **ratatui**: List widget with ListState for stateful navigation

---

## 9. Success Metrics

| Metric | Target |
|--------|--------|
| Time to find workflow | <3 keystrokes with fuzzy search |
| Time to navigate to .nika | 1 keystroke (`~`) |
| Learning curve | Familiar to vim/ranger users |
| Hidden files toggle | Single key (`.`) |
| Preview toggle | Single key (`Tab`) |

---

## Appendix: Full Keybinding Reference

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  EXPLORER VIEW - KEYBOARD SHORTCUTS                                           ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  NAVIGATION                          FILE OPERATIONS                          ║
║  ───────────                         ───────────────                          ║
║  j / ↓      Move down                a         Create file                    ║
║  k / ↑      Move up                  A         Create folder                  ║
║  h / ←      Go to parent             r         Rename                         ║
║  l / → / ⏎  Open/expand              d         Delete (confirm)               ║
║  gg         Go to first              y         Copy path                      ║
║  G          Go to last               o         Open external                  ║
║  Ctrl+u     Page up                                                           ║
║  Ctrl+d     Page down                PREVIEW                                  ║
║                                      ───────                                  ║
║  SEARCH & FILTER                     Tab       Toggle DAG/YAML                ║
║  ───────────────                     p         Toggle preview                 ║
║  /          Start fuzzy search       z         Zoom preview                   ║
║  .          Toggle hidden files                                               ║
║  Esc        Clear/cancel             QUICK JUMP                               ║
║                                      ──────────                               ║
║  SELECTION                           ~         Go to .nika/                   ║
║  ─────────                           w         Go to workflows/               ║
║  Space      Toggle select            t         Go to traces/                  ║
║  v          Visual select            B         Bookmarks                      ║
║  m          Context menu                                                      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```
