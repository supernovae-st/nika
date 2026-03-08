# Terminal IDE UX Patterns Research

**Date:** 2026-03-08
**Author:** Claude (Research Agent)
**Purpose:** Inform Nika TUI v0.22+ development with world-class UX patterns

---

## Executive Summary

This research analyzes UX patterns from the most successful terminal IDEs (Helix, Kakoune, Neovim, Zed terminal mode) and modern TUI applications. The goal is to identify actionable patterns for Nika's 4-view TUI architecture.

**Key Finding:** The best terminal IDEs share common principles: keyboard-first navigation, modal editing with clear visual feedback, and progressive disclosure of complexity.

---

## Part 1: Best Terminal IDEs Analyzed

### 1.1 Helix Editor

**Why it excels:**
- **Selection-first model**: Actions operate on existing selections (select -> act)
- **Built-in LSP**: Language intelligence without plugins
- **Tree-sitter integration**: Structural editing and navigation
- **Space mode**: Discoverable commands via `Space` prefix

**Key UX innovations:**
```
Selection-first workflow:
  vim:   d3w  (delete 3 words - action first)
  helix: 3w d (select 3 words, then delete - selection first)

Benefits:
- Visual feedback BEFORE destructive action
- Undo granularity matches user intent
- Easier to learn (see what you're affecting)
```

**Nika takeaway:** Consider selection-first for task/file operations. Show preview before execution.

### 1.2 Kakoune Editor

**Why it excels:**
- **Multiple selections**: First-class multi-cursor support
- **Orthogonal design**: Commands compose predictably
- **Client-server architecture**: Multiple views of same buffer
- **Mode line info**: Context-aware status display

**Key UX patterns:**
```
Orthogonal keys:
  w  = select word
  W  = extend selection to word
  <a-w> = select inner word

  Pattern: base key + modifier = variation
```

**Nika takeaway:** Design keybindings orthogonally. Same base key, modifiers for variations.

### 1.3 Neovim

**Why it excels:**
- **Lua ecosystem**: Fast, composable plugins
- **Floating windows**: Modern UI overlays
- **Telescope**: Fuzzy finding paradigm
- **Which-key**: Discoverable keybindings

**Key UX patterns:**
```
Which-key pattern:
  Press <leader> (space) -> wait 300ms -> popup shows options

  [ ] g  - Git commands
  [ ] f  - Find files
  [ ] s  - Search text
  [ ] b  - Buffer operations

Benefits:
- Zero memorization for beginners
- Fast execution for experts (no wait if you know the key)
- Self-documenting interface
```

**Nika takeaway:** Implement which-key style command discovery. Essential for 4-view architecture.

### 1.4 Zed Editor (Terminal Mode)

**Why it excels:**
- **GPU-accelerated**: 120fps rendering
- **Collaborative-first**: Real-time multiplayer
- **AI integration**: Built-in assistant panel
- **Minimal chrome**: Content-focused design

**Key UX patterns:**
```
Panel architecture:
  Left:   Project tree (collapsible)
  Center: Editor (tabs)
  Right:  Assistant/Terminal (collapsible)
  Bottom: Status + Command palette

Transitions:
- Panels slide in/out (100ms ease-out)
- Focus follows panel visibility
- Keyboard shortcuts toggle panels
```

**Nika takeaway:** Panel-based layout with smooth transitions. Nika's 4-view maps well to this.

---

## Part 2: Modern Terminal UI Patterns

### 2.1 Fuzzy Finders

**The standard:** fzf-style incremental search

```
Components:
  [Input field with cursor]
  [Match count: 42/1337]
  [Results list with highlighting]
  [Preview pane (optional)]

Behavior:
- Start typing = immediate filtering
- No "search" button needed
- Results update on every keystroke
- Highlight matching characters

Scoring algorithm (fzf):
1. Exact match > prefix match > subsequence match
2. Consecutive characters score higher
3. Word boundaries score higher
4. Recent items score higher
```

**Implementation for Nika:**
```rust
// Fuzzy matcher for file browser and command palette
struct FuzzyMatcher {
    query: String,
    items: Vec<FuzzyItem>,
    selected: usize,
    preview_enabled: bool,
}

impl FuzzyMatcher {
    fn score(&self, item: &str) -> i32 {
        // Implement fzf-style scoring
        // - Consecutive bonus
        // - Word boundary bonus
        // - Case-insensitive matching
    }
}
```

### 2.2 Command Palettes

**Best practices (VS Code / Zed style):**

```
Invocation: Ctrl+Shift+P or Cmd+K
Display: Floating overlay, 60% viewport width

Structure:
  > [fuzzy search input]

  Recently Used
    [ ] Open File...
    [ ] Save All

  Commands
    [ ] Git: Commit
    [ ] Format Document
    [ ] Toggle Sidebar

Features:
- Prefix filtering: ">" for commands, "@" for symbols, ":" for go to line
- Keybinding hints shown inline
- Category grouping
- Recent commands at top
```

**Nika implementation:**
```yaml
Command Palette Structure:
  ">"  - All commands
  "@"  - Jump to task (by ID)
  ":"  - Go to line
  "/"  - Search in file
  "!"  - Shell command
  "#"  - Jump to section (schema, tasks, flows, mcp)
```

### 2.3 Tree Navigation (VS Code Style)

**Key behaviors:**

```
Keyboard:
  j/Down    - Next item
  k/Up      - Previous item
  h/Left    - Collapse folder / Go to parent
  l/Right   - Expand folder / Enter file
  Enter     - Open file
  Space     - Toggle expand
  /         - Start search

Visual feedback:
  [>] src/              <- Collapsed folder (chevron right)
  [v] src/              <- Expanded folder (chevron down)
      [*] main.rs       <- Modified file (dot indicator)
      [ ] lib.rs        <- Unchanged file

  Selection: Full-width highlight
  Focus ring: Border on focused panel

Indentation:
  Root level: 0 indent
  Each level: +2 spaces (or use Unicode tree chars)
```

**Nika-specific additions:**
```
File icons by extension:
  .nika.yaml  ->  (workflow)
  .yaml       ->  (config)
  .md         ->  (docs)
  .rs         ->  (code)

Workflow-specific indicators:
  [>] workflows/
      [ ] build.nika.yaml      5 tasks
      [*] deploy.nika.yaml     3 tasks (modified)
      [!] broken.nika.yaml     2 errors
```

---

## Part 3: Keyboard-Driven Navigation

### 3.1 Modal vs Modeless

**Spectrum:**

```
Pure Modal (Vim)          Hybrid (Helix)          Modeless (Nano)
|-------------------------|--------------------------|
  - Normal mode            - Normal mode             - Single mode
  - Insert mode            - Select mode             - Ctrl chords
  - Visual mode            - Insert mode
  - Command mode

Trade-offs:
  Modal: Higher ceiling, steeper curve
  Modeless: Lower ceiling, immediate productivity
```

**Recommendation for Nika:**
Hybrid approach - Normal mode for navigation, Insert mode only in editor panel.

```
Nika Modal Design:
  Normal Mode (default):
    - Navigate views: 1-4
    - Navigate panels: Tab, Shift+Tab
    - Navigate items: j/k or arrows
    - Execute actions: Enter, Space

  Insert Mode (editor only):
    - All keys insert text
    - Esc returns to Normal

  Command Mode:
    - : prefix (like vim)
    - Colon commands for advanced operations
```

### 3.2 Keybinding Conventions

**Universal standards (violate at your peril):**

```
MUST FOLLOW:
  Ctrl+C    - Cancel / Copy (context-dependent)
  Ctrl+V    - Paste
  Ctrl+Z    - Undo
  Ctrl+S    - Save
  Ctrl+Q    - Quit
  Ctrl+W    - Close tab/panel
  Escape    - Cancel / Exit mode
  Tab       - Next field / Indent
  Enter     - Confirm / Execute

COMMONLY EXPECTED:
  Ctrl+P    - Command palette / Quick open
  Ctrl+F    - Find
  Ctrl+G    - Go to line
  Ctrl+/    - Toggle comment
  Ctrl+B    - Toggle sidebar
  F1        - Help
  F5        - Run / Debug
```

**Nika-specific keybindings:**
```
View switching:
  1 - Studio view
  2 - Runner view
  3 - Chat view
  4 - Settings view

Panel navigation (in Studio):
  Tab       - Next panel (Browser -> Editor -> DAG)
  Shift+Tab - Previous panel
  Ctrl+1    - Focus Browser
  Ctrl+2    - Focus Editor
  Ctrl+3    - Focus DAG

Workflow actions:
  F5        - Run workflow
  F6        - Validate workflow
  Ctrl+R    - Run current task
  Ctrl+Enter - Run from cursor
```

### 3.3 Vim-Style Motion Grammar

**Pattern: [count][motion/object]**

```
Motions (navigation):
  h/j/k/l   - Character movement
  w/b/e     - Word movement
  0/$       - Line start/end
  gg/G      - File start/end
  Ctrl+d/u  - Half-page down/up

Objects (selections):
  iw        - Inner word
  aw        - A word (with whitespace)
  i"        - Inner quotes
  a"        - A quoted string
  ip        - Inner paragraph

Actions:
  d         - Delete
  y         - Yank (copy)
  c         - Change (delete + insert)
  v         - Visual select
```

**Nika adaptation for tasks:**
```
Task motions:
  ]t        - Next task
  [t        - Previous task
  ]f        - Next flow
  [f        - Previous flow
  gt        - Go to task (fuzzy)
  gd        - Go to dependency

Task actions (Normal mode):
  dd        - Delete task
  yy        - Yank task
  p         - Paste task
  r         - Rename task
  x         - Toggle task enabled
```

---

## Part 4: Specific UX Patterns

### 4.1 Split Panes and Tab Management

**Split pane patterns:**

```
Creation:
  Ctrl+\    - Vertical split
  Ctrl+-    - Horizontal split

Navigation:
  Ctrl+h/j/k/l - Move focus between panes (vim-style)
  Ctrl+w then h/j/k/l - Alternative (vim compat)

Resizing:
  Ctrl+Shift+h/l - Resize horizontal
  Ctrl+Shift+j/k - Resize vertical
  Ctrl+= - Equalize panes

Closing:
  Ctrl+w q  - Close current pane
  Ctrl+w o  - Close other panes (zoom)
```

**Tab management:**

```
Navigation:
  Ctrl+Tab      - Next tab
  Ctrl+Shift+Tab - Previous tab
  Ctrl+1-9      - Jump to tab N
  Alt+Left/Right - Adjacent tabs

Operations:
  Ctrl+T    - New tab
  Ctrl+W    - Close tab
  Ctrl+Shift+T - Reopen closed tab

Tab bar design:
  [x] file1.yaml | [ ] file2.yaml | [+]
       ^active        ^inactive    ^new

  - Show close button on hover
  - Truncate long names with ellipsis
  - Show modified indicator (dot)
```

**Nika implementation:**
```
Studio view pane layout:

  +------------------+------------------+------------------+
  |     BROWSER      |      EDITOR      |    DAG PREVIEW   |
  |                  |                  |                  |
  |  [ ] workflows/  |  workflow: test  |     +------+     |
  |    [ ] test.yaml |                  |     | step1 |    |
  |    [ ] prod.yaml |  tasks:          |     +---+---+    |
  |                  |    - id: step1   |         |        |
  |                  |      infer: ... .|     +---v---+    |
  |                  |                  |     | step2 |    |
  |                  |                  |     +-------+    |
  +------------------+------------------+------------------+
       20-30%             40-50%             20-30%

  Ratios (Ctrl+] to cycle):
    Balanced:  30% | 40% | 30%
    Editor+:   20% | 60% | 20%
    Browser+:  40% | 40% | 20%
    DAG+:      20% | 40% | 40%
```

### 4.2 Status Bar Design

**Anatomy of an excellent status bar:**

```
+------------------------------------------------------------------------+
| MODE | LOCATION | CONTEXT INFO | ---------- | DIAGNOSTICS | INDICATORS |
+------------------------------------------------------------------------+

Left side (stable):
  [NORMAL] src/main.rs:42:15 (function: main)

Center (context):
  Recording macro q | Search: /pattern | Replacing: old->new

Right side (indicators):
  [2W 1E] | LF | UTF-8 | Rust | Git: main*

Visual encoding:
  - Mode badge: colored background
  - Errors: red
  - Warnings: yellow
  - Git dirty: asterisk
```

**Nika status bar:**
```
+------------------------------------------------------------------------+
| [STUDIO] workflow.nika.yaml | task: generate | ----- | [!2] | Claude |
+------------------------------------------------------------------------+

Modes:
  [STUDIO]  - Editing (cyan)
  [RUNNING] - Executing (green, animated)
  [CHAT]    - Conversation (purple)
  [ERROR]   - Failed (red)

Context info:
  - Current file
  - Current task (when editor focused)
  - MCP server status

Right indicators:
  - Diagnostic count [!2] or [OK]
  - Provider name
  - Token usage (in chat)
```

### 4.3 Search and Replace UX

**Best practices:**

```
Search (Ctrl+F):
  +-----------------------------------------+
  | [x] Case | [x] Regex | [ ] Word | Aa    |
  +-----------------------------------------+
  | pattern_here                      [3/42] |
  +-----------------------------------------+

  - Incremental (results update as you type)
  - Match count visible
  - Option toggles (case, regex, whole word)
  - Visual match highlighting
  - Navigate: F3/Enter next, Shift+F3 previous

Replace (Ctrl+H):
  +-----------------------------------------+
  | Search:  pattern_here                   |
  | Replace: replacement_here               |
  +-----------------------------------------+
  | [Replace] [Replace All] [Preview]       |
  +-----------------------------------------+

  - Preview mode shows diff
  - Incremental replace (one at a time)
  - Undo for replace all
  - Preserve case option
```

**Nika-specific search:**
```
Task search (gt or /):
  - Search by task ID
  - Search by verb (infer:, exec:, etc.)
  - Search by MCP tool

Flow search:
  - Find dependencies
  - Find dependents

Cross-file search (Ctrl+Shift+F):
  - Search all .nika.yaml files
  - Group results by file
  - Preview in context
```

### 4.4 Git Integration

**Terminal IDE patterns:**

```
Status indicators in tree:
  M  - Modified
  A  - Added (staged)
  ?  - Untracked
  D  - Deleted
  R  - Renamed
  C  - Conflict

Gutter signs in editor:
  |  - Added line (green bar)
  ~  - Modified line (blue bar)
  _  - Deleted line (red triangle)

Inline blame:
  ... existing code ...  // John Doe, 3 days ago
  ... new code ...       // <uncommitted>

Commands:
  :Git status  - Show status
  :Git diff    - Show diff
  :Git commit  - Open commit interface
  :Git blame   - Toggle inline blame
```

**Nika git integration:**
```
Workflow-aware git:
  - Show workflow version history
  - Diff workflow schemas
  - Track which tasks changed

Git status in browser:
  [ ] workflows/
      [M] build.nika.yaml    - Modified
      [?] new.nika.yaml      - Untracked

Quick actions:
  gs  - Git status overlay
  gd  - Git diff current file
  gc  - Git commit (opens message input)
```

---

## Part 5: Top 10 UX Patterns to Implement

Based on this research, here are the highest-impact patterns for Nika v0.22+:

### 1. Which-Key Command Discovery

```
Priority: HIGH
Effort: MEDIUM

When user presses Space (leader), show popup after 300ms:

  +----------------------------------+
  | [f] Find file                    |
  | [s] Search text                  |
  | [t] Go to task                   |
  | [r] Run workflow                 |
  | [g] Git commands...              |
  | [w] Window commands...           |
  +----------------------------------+

Implementation:
- Timer on leader key
- Popup with keybinding hints
- Nested menus for categories
```

### 2. Fuzzy File Finder (Ctrl+P)

```
Priority: HIGH
Effort: LOW (use existing fuzzy logic)

Quick open for .nika.yaml files:

  +------------------------------------------+
  | > build                                   |
  +------------------------------------------+
  | workflows/build.nika.yaml        5 tasks |
  | workflows/build-prod.nika.yaml   8 tasks |
  | ci/build-docker.nika.yaml        3 tasks |
  +------------------------------------------+

Features:
- Task count preview
- Recent files first
- Show validation status
```

### 3. Selection-First Task Operations

```
Priority: MEDIUM
Effort: MEDIUM

Before deleting/moving tasks, select them first:

  Current task highlighted
  -> Press 'd' -> Confirmation popup
  -> Press 'y' -> Delete

  Multi-select with Shift+j/k
  -> All selected tasks operate together
```

### 4. Inline Validation Diagnostics

```
Priority: HIGH
Effort: MEDIUM

Show errors inline, not just in status bar:

  tasks:
    - id: broken_task
      infer: missing quote       <- [!] Expected closing quote
      depends_on: [nonexistent]  <- [!] Unknown task: nonexistent

Gutter:
  [!] Line 5  - Error
  [~] Line 8  - Warning
  [i] Line 12 - Info
```

### 5. Smooth Panel Transitions

```
Priority: MEDIUM
Effort: LOW

Animate panel focus changes:

  - Border color transition (100ms)
  - Panel resize animation (200ms ease-out)
  - Fade effect on background panels
```

### 6. Context-Aware Status Bar

```
Priority: MEDIUM
Effort: LOW

Show different info based on focused panel:

  Browser focused:
    [STUDIO] 42 workflows | 12 modified | ----- | Claude

  Editor focused:
    [STUDIO] build.nika.yaml:15:3 | task: deploy | schema @0.9

  DAG focused:
    [STUDIO] 5 tasks | 4 flows | Critical path: 3 steps
```

### 7. Task Quick Actions (Space Menu)

```
Priority: HIGH
Effort: MEDIUM

Context menu for tasks:

  With cursor on a task:
  Space ->
    +---------------------------+
    | [r] Run this task         |
    | [d] Delete task           |
    | [e] Edit in popup         |
    | [c] Copy task             |
    | [gt] Go to dependencies   |
    | [gd] Go to dependents     |
    +---------------------------+
```

### 8. Breadcrumb Navigation

```
Priority: LOW
Effort: LOW

Show location context:

  workflows/ > build.nika.yaml > tasks > [3] deploy > infer

Click/keyboard to jump to any level.
```

### 9. Split View Presets

```
Priority: MEDIUM
Effort: LOW

Quick layout switching:

  Ctrl+Alt+1: Browser only (full width)
  Ctrl+Alt+2: Editor only (full width)
  Ctrl+Alt+3: Editor + DAG (50/50)
  Ctrl+Alt+4: All three (30/40/30)
  Ctrl+Alt+5: Editor + Runner (run mode)
```

### 10. Undo/Redo Visual Feedback

```
Priority: LOW
Effort: LOW

Show undo/redo status:

  After Ctrl+Z:
    "Undo: Delete task 'deploy'" (flash message, 1s)

  After Ctrl+Y:
    "Redo: Delete task 'deploy'" (flash message, 1s)

  Show undo stack depth in status bar when available:
    [Undo: 5] [Redo: 2]
```

---

## Part 6: Common Pitfalls to Avoid

### 6.1 Mode Confusion

```
PITFALL: User doesn't know what mode they're in
SOLUTION: Always show mode prominently

Bad:
  (no mode indicator)

Good:
  [NORMAL] visible in status bar
  Visual mode changes cursor/border color
  Insert mode shows cursor blinking
```

### 6.2 Hidden Features

```
PITFALL: Users never discover advanced features
SOLUTION: Progressive disclosure

Bad:
  Features only documented in manual

Good:
  - Which-key popups
  - Tooltips on hover
  - ? shows contextual help
  - First-run tutorial
```

### 6.3 Inconsistent Keybindings

```
PITFALL: Same key does different things in different contexts
SOLUTION: Orthogonal design

Bad:
  Enter = confirm in dialog, newline in editor, run in browser

Good:
  Enter = primary action (always)
  Ctrl+Enter = run/execute (always)
  Esc = cancel/exit (always)
```

### 6.4 Modal Traps

```
PITFALL: User gets stuck in a mode
SOLUTION: Esc always exits

Bad:
  Some modes require Ctrl+C to exit

Good:
  Esc works in EVERY context
  Esc Esc (double) returns to Normal from anywhere
```

### 6.5 Slow Feedback

```
PITFALL: Operations feel sluggish
SOLUTION: Immediate visual response

Bad:
  Click button -> 500ms delay -> action happens

Good:
  Click button -> immediate visual change -> action completes
  Use spinners/progress for long operations
```

---

## Part 7: Color and Theming Best Practices

### 7.1 Semantic Colors

```
Use colors for meaning, not decoration:

  Error:   Red (#f85149)
  Warning: Yellow (#d29922)
  Success: Green (#3fb950)
  Info:    Blue (#58a6ff)
  Muted:   Gray (#8b949e)

Task verb colors (Nika-specific):
  infer:  Purple (AI/generation)
  exec:   Orange (shell/system)
  fetch:  Blue (network)
  invoke: Cyan (MCP)
  agent:  Green (autonomous)
```

### 7.2 Contrast Requirements

```
WCAG AA minimum contrast ratios:
  Text on background: 4.5:1
  Large text: 3:1
  UI components: 3:1

Nika themes:
  Light theme: #1f2328 on #ffffff (21:1)
  Dark theme:  #e6edf3 on #0d1117 (15:1)
  Solarized:   #657b83 on #fdf6e3 (7:1)
```

### 7.3 Focus Indicators

```
MUST be visible for accessibility:

  Focused panel: 2px solid border (#58a6ff)
  Focused item:  Background highlight + border

  Don't rely on color alone:
    - Use border/underline for focused
    - Use bold for selected
    - Use icon for status
```

### 7.4 Reduced Motion

```
Respect user preference:

  @media (prefers-reduced-motion: reduce) {
    * { animation: none !important; }
  }

In Rust/TUI:
  - Check system preference
  - Provide config option
  - Skip animations when disabled
```

---

## Part 8: Implementation Roadmap for Nika

### Phase 1: Foundation (v0.22)

```
[ ] Which-key command discovery
[ ] Fuzzy file finder (Ctrl+P)
[ ] Consistent keybindings audit
[ ] Mode indicator polish
```

### Phase 2: Navigation (v0.23)

```
[ ] Task-aware motions (]t, [t, gt)
[ ] Selection-first operations
[ ] Breadcrumb navigation
[ ] Split view presets
```

### Phase 3: Polish (v0.24)

```
[ ] Inline diagnostics
[ ] Smooth panel transitions
[ ] Context-aware status bar
[ ] Undo/redo feedback
```

### Phase 4: Advanced (v0.25+)

```
[ ] Git integration
[ ] Multi-cursor support
[ ] Macro recording
[ ] Custom keybinding config
```

---

## Appendix A: Keybinding Reference

### Global (all views)

```
1/2/3/4     Switch views (Studio/Runner/Chat/Settings)
?           Show help
Ctrl+Q      Quit
Ctrl+P      Command palette
Ctrl+K      Quick actions
Esc         Cancel/Exit mode
```

### Studio View

```
Tab         Next panel
Shift+Tab   Previous panel
Ctrl+S      Save
Ctrl+Z      Undo
Ctrl+Y      Redo
F5          Run workflow
F6          Validate
/           Search in file
gt          Go to task
```

### Editor Panel

```
i           Enter insert mode
Esc         Exit insert mode
j/k         Move down/up
w/b         Word forward/back
gg/G        Start/end of file
dd          Delete line
yy          Yank line
p           Paste
u           Undo
Ctrl+R      Redo
```

### Browser Panel

```
j/k         Navigate
l/Enter     Open/Expand
h           Collapse/Parent
Space       Toggle expand
/           Filter
a           New file
d           Delete
r           Rename
```

### DAG Panel

```
j/k         Navigate tasks
Enter       Focus in editor
Tab         Cycle through tasks
Space       Toggle task details
r           Run task
```

---

## Appendix B: Resources

### Terminal UI Crates (Rust)

```
ratatui     - TUI framework (Nika's foundation)
crossterm   - Terminal manipulation
tui-textarea - Text editor widget
tui-tree-widget - Tree view widget
fuzzy-matcher - Fuzzy search
```

### Design References

- Helix documentation: https://docs.helix-editor.com/
- Kakoune wiki: https://github.com/mawww/kakoune/wiki
- Neovim UI conventions: https://neovim.io/doc/user/ui.html
- VS Code UX guidelines: https://code.visualstudio.com/api/ux-guidelines
- Zed blog (design decisions): https://zed.dev/blog

### Accessibility

- WCAG 2.1 guidelines: https://www.w3.org/WAI/WCAG21/quickref/
- Terminal accessibility: https://github.com/a11y-guidelines/a11y-guidelines.github.io

---

## Conclusion

The best terminal IDEs succeed through:

1. **Keyboard-first design** with discoverable commands
2. **Modal editing** with clear visual feedback
3. **Orthogonal keybindings** that compose predictably
4. **Progressive disclosure** of advanced features
5. **Immediate feedback** for all operations

For Nika v0.22+, prioritize:
- Which-key command discovery (highest impact)
- Fuzzy file finder (table stakes)
- Consistent keybindings (prevents frustration)
- Inline diagnostics (workflow-specific value)

The 4-view architecture is already well-designed. Focus on making each view feel polished and discoverable.

---

*Research compiled by Claude (Opus 4.5) for SuperNovae Studio*
*Last updated: 2026-03-08*
