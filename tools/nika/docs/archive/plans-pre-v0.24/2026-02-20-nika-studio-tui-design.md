# NIKA STUDIO TUI Design

**Date:** 2026-02-20
**Status:** Design approved, pending implementation
**Author:** Thibaut + Claude (brainstorm session)

## Overview

Unified TUI experience combining workflow browsing, editing, and execution monitoring with an AI chat assistant.

## CLI Simplification

```bash
nika                    # Launch TUI (Home View)
nika file.nika.yaml     # Headless run (no TUI)
nika -i file.nika.yaml  # Interactive run (Monitor View)
nika studio             # Explicit TUI launch
```

## Architecture

### Three Views

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│   HOME ──────► STUDIO ──────► MONITOR                           │
│   (Browse)     (Edit)         (Run)                             │
│                                                                 │
│   [Enter]      [F5]           [q] back                          │
│   [e] edit     [q] back       [r] restart                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### TuiView Enum (new)

```rust
pub enum TuiView {
    Home,    // File browser + history + preview
    Studio,  // Editor + tasks + validation
    Monitor, // Execution monitor (existing)
}
```

### ChatMode Enum (new)

One ChatPanel component with three contextual behaviors:

```rust
pub enum ChatMode {
    Create,  // Home: "Create a workflow that..."
    Edit,    // Studio: "Add retry to fetch task"
    Debug,   // Monitor: "Why did this task fail?"
}
```

| Mode | Context | Actions |
|------|---------|---------|
| Create | File browser, selected file preview | `/save <name>` generates new workflow |
| Edit | Current YAML, cursor position, errors | Applies diffs directly to editor |
| Debug | Events, traces, task outputs | Analyzes execution, suggests fixes |

## View Layouts

### HOME VIEW

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ NIKA STUDIO                                                    [?] Help   [q] ×│
├───────────────────────────────────┬─────────────────────────────────────────────┤
│ 📂 WORKFLOWS                      │ 📄 PREVIEW                                  │
│                                   │                                             │
│ Tree view of .nika.yaml files     │ YAML syntax highlighted preview             │
│ with folder navigation            │ of selected file                            │
│                                   │                                             │
│ [↑↓] navigate                     │ Read-only                                   │
│ [Enter] open folder / run file    │                                             │
│ [e] open in Studio                │                                             │
│                                   │                                             │
├───────────────────────────────────┴─────────────────────────────────────────────┤
│ 📜 HISTORY (recent)  ────────────────────────────────────────── [h] toggle     │
│  • file1.nika.yaml (2min ago ✓)  • file2.nika.yaml (1h ago ✗)  • file3...      │
├─────────────────────────────────────────────────────────────────────────────────┤
│ 🤖 CHAT [c] ─────────────────────────────────────────────── ChatMode::Create   │
│                                                                                 │
│ Conversational workflow creation                                                │
│ "/save <name>" to generate file                                                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
│ [↑↓] Navigate  [Enter] Open/Run  [e] Edit in Studio  [c] Chat  [h] History     │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Layout (ratatui):**
```
Vertical [
  Header (1 line)
  Horizontal [
    Tree (40%)
    Preview (60%)
  ] (flex)
  History (3 lines, toggleable)
  Chat (30%, toggleable)
  StatusBar (1 line)
]
```

### STUDIO VIEW

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ NIKA STUDIO › filename.nika.yaml                          [F5] Run   [q] Back  │
├──────────────────────────────────────┬──────────────────────────────────────────┤
│ 📝 EDITOR                            │ 📋 TASKS                                 │
│                                      │                                          │
│ YAML editor with:                    │ DAG visualization of tasks               │
│ - Line numbers                       │ with status indicators                   │
│ - Syntax highlighting                │                                          │
│ - Error underlines                   │ Validation panel:                        │
│ - vim keybindings                    │ - Schema errors                          │
│                                      │ - Warnings                               │
│ [i] insert mode                      │ - Suggestions                            │
│ [Esc] normal mode                    │                                          │
│                                      │                                          │
├──────────────────────────────────────┴──────────────────────────────────────────┤
│ 🤖 CHAT [c] ─────────────────────────────────────────────── ChatMode::Edit     │
│                                                                                 │
│ Context-aware editing assistance                                                │
│ Applies diffs directly to editor                                                │
│ [Ctrl+Z] to undo changes                                                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
│ [i] Insert  [Esc] Normal  [F5] Run  [c] Chat  [Ctrl+S] Save                     │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Layout (ratatui):**
```
Vertical [
  Header (1 line)
  Horizontal [
    Editor (60%)
    Tasks (40%)
  ] (flex)
  Chat (30%, toggleable)
  StatusBar (1 line)
]
```

### MONITOR VIEW

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ NIKA MONITOR › workflow-name ▶ Running (2/3 tasks)                [q] Stop     │
├──────────────────────────────────────┬──────────────────────────────────────────┤
│ 🎯 MISSION CONTROL                   │ 🔀 DAG                                   │
│                                      │                                          │
│ Task list with status                │ Animated DAG visualization               │
│ Progress bar                         │ Real-time status updates                 │
│ Event stream                         │                                          │
│                                      │                                          │
├──────────────────────────────────────┼──────────────────────────────────────────┤
│ 🌐 NOVANET                           │ 🧠 REASONING                             │
│                                      │                                          │
│ MCP tool calls                       │ LLM thinking/reasoning                   │
│ Request/response pairs               │ Token usage                              │
│                                      │                                          │
├──────────────────────────────────────┴──────────────────────────────────────────┤
│ 🤖 CHAT [c] ─────────────────────────────────────────────── ChatMode::Debug    │
│                                                                                 │
│ Debug assistance                                                                │
│ Analyzes events, traces, outputs                                                │
│ Suggests fixes for failures                                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
│ [1-4] Panels  [Tab] Cycle  [c] Chat  [Space] Pause/Resume  [r] Restart         │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Layout (ratatui):**
```
Vertical [
  Header (1 line)
  Horizontal [
    Vertical [Mission (50%), NovaNet (50%)]
    Vertical [DAG (50%), Reasoning (50%)]
  ] (flex)
  Chat (30%, toggleable)
  StatusBar (1 line)
]
```

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `q` | Quit / Back |
| `?` | Help overlay |
| `c` | Toggle Chat |
| `Esc` | Close overlay / Exit mode |

### HOME VIEW

| Key | Action |
|-----|--------|
| `↑↓` / `j/k` | Navigate tree |
| `Enter` | Open folder / Run workflow |
| `e` | Open in Studio |
| `h` | Toggle history bar |
| `/` | Search files |

### STUDIO VIEW

| Key | Action |
|-----|--------|
| `i` | Insert mode (editor) |
| `Esc` | Normal mode |
| `F5` | Run workflow |
| `Ctrl+S` | Save file |
| `Ctrl+Z` | Undo |
| `Tab` | Switch Editor ↔ Tasks |

### MONITOR VIEW

| Key | Action |
|-----|--------|
| `1-4` | Focus panel |
| `Tab` | Cycle panels |
| `Space` | Pause/Resume |
| `r` | Restart workflow |

## Component Architecture

```
src/tui/
├── mod.rs              # Entry point, TuiView enum
├── app.rs              # Event loop, Action enum (extend)
├── state.rs            # TuiState (extend with view field)
├── theme.rs            # NovaNet colors (keep)
├── views/
│   ├── mod.rs          # View trait
│   ├── home.rs         # NEW: HomeView
│   ├── studio.rs       # NEW: StudioView
│   └── monitor.rs      # EXISTING: MonitorView (adapt)
├── components/
│   ├── mod.rs          # Component trait
│   ├── tree.rs         # NEW: File tree browser
│   ├── preview.rs      # NEW: YAML preview (read-only)
│   ├── editor.rs       # NEW: YAML editor (tui-textarea)
│   ├── tasks.rs        # NEW: Task DAG mini-view
│   ├── history.rs      # EXISTING: Adapt from standalone.rs
│   ├── chat.rs         # NEW: ChatPanel with ChatMode
│   └── status_bar.rs   # NEW: Contextual hints
└── standalone.rs       # DEPRECATE: Merge into home.rs
```

## Dependencies

```toml
[dependencies]
# Existing
ratatui = "0.29"
crossterm = "0.28"

# New
tui-textarea = "0.7"  # Editor component
syntect = "5"         # Syntax highlighting
```

## Implementation Phases

### Phase 1: Foundation
- [ ] Add `TuiView` enum to state
- [ ] Create view navigation (Home ↔ Studio ↔ Monitor)
- [ ] Implement StatusBar with contextual hints
- [ ] CLI arg handling for new commands

### Phase 2: HOME VIEW
- [ ] Tree browser component (from standalone.rs)
- [ ] YAML preview panel
- [ ] History bar (horizontal, toggleable)
- [ ] Basic navigation

### Phase 3: STUDIO VIEW
- [ ] Integrate tui-textarea for editor
- [ ] YAML syntax highlighting
- [ ] Task DAG mini-view
- [ ] Real-time validation
- [ ] Save/load workflow

### Phase 4: CHAT Integration
- [ ] ChatPanel component with ChatMode
- [ ] Toggle with [c] in all views
- [ ] ChatMode::Create (prompt → workflow generation)
- [ ] ChatMode::Edit (prompt → diff application)
- [ ] ChatMode::Debug (prompt → execution analysis)

### Phase 5: Polish
- [ ] Vim keybindings in editor
- [ ] Search functionality
- [ ] Error underlines in editor
- [ ] Smooth transitions between views

## Open Questions

1. **Chat backend**: Local LLM vs API call? → Decision: Use Nika's existing `infer:` with Claude
2. **History persistence**: Keep `~/.nika/history.json`? → Yes
3. **Theme**: Dark mode only or light mode too? → Start with dark, add light later

## References

- Lazygit: Panel navigation, vim keybindings
- Helix: Status line, command palette
- Zellij: Discoverability patterns
- tui-textarea: Editor component
