# Nika TUI Editor — Full LSP Integration + UX Upgrade

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Wire the existing nika-lsp-core (50+ completions, hover docs, goto-def, 6 code actions) into the TUI editor. Connect the tree-sitter highlighter. Add diagnostic gutter + undercurl. Add missing navigation keys. Transform the editor from a basic textarea into a real IDE experience.

**Architecture:** In-process LSP — call `nika_lsp_core::DefaultHandler` directly from the TUI (same pattern as DiagnosticsEngine). No client/server stdio. Feature-gated behind `#[cfg(feature = "lsp")]`.

**Tech Stack:** Rust, ratatui 0.30, nika-lsp-core, tree-sitter-yaml, crossterm (undercurl support)

---

## Phase 1: Diagnostic Gutter + Undercurl (visual feedback)

### Task 1.1: Render diagnostic gutter icons

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs:2273-2349` (render_editor main loop)

**Step 1: Add diagnostic gutter column**

In the render_editor() line loop (line 2273), after git gutter rendering (lines 2290-2302), add diagnostic gutter:

```rust
// After git gutter span, before line number:
let diag_gutter = if let Some(diag) = self.diagnostics.most_severe_on_line(i) {
    match diag.severity {
        DiagnosticSeverity::Error => Span::styled("● ", Style::default().fg(Color::Rgb(243, 139, 168))),   // Catppuccin Red
        DiagnosticSeverity::Warning => Span::styled("▲ ", Style::default().fg(Color::Rgb(249, 226, 175))), // Catppuccin Yellow
        DiagnosticSeverity::Hint => Span::styled("◆ ", Style::default().fg(Color::Rgb(137, 180, 250))),    // Catppuccin Blue
    }
} else {
    Span::raw("  ") // 2-char gutter placeholder
};
spans.push(diag_gutter);
```

Insert this AFTER the git gutter span and BEFORE the line number span in the spans vector construction.

**Step 2: Build and verify**

Run: `cd tools/nika && cargo build --features tui 2>&1 | tail -3`

**Step 3: Commit**

```
fix(tui): render diagnostic gutter icons in editor
```

---

### Task 1.2: Render diagnostic underlines (undercurl)

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs:2273-2349`

**Step 1: Add underline styling for diagnostic spans**

After the line content is rendered, apply underline styles for diagnostics on that line:

```rust
// After line is built but before final render, apply diagnostic underlines
for diag in self.diagnostics.diagnostics_for_line(i) {
    let underline_color = match diag.severity {
        DiagnosticSeverity::Error => Color::Rgb(243, 139, 168),   // Red
        DiagnosticSeverity::Warning => Color::Rgb(249, 226, 175), // Yellow
        DiagnosticSeverity::Hint => Color::Rgb(137, 180, 250),    // Blue
    };
    // Apply underline to the affected character range
    // diag.start_col and diag.end_col are 0-indexed within the line
    let gutter_width = 8; // git(1) + diag(2) + linenum(5)
    let start_x = content_area.x + gutter_width as u16 + diag.start_col as u16;
    let end_x = content_area.x + gutter_width as u16 + diag.end_col.min(line.len()) as u16;
    let y = content_area.y + (i - self.buffer.scroll_offset()) as u16;

    for x in start_x..end_x.min(content_area.right()) {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
            cell.set_style(
                cell.style()
                    .add_modifier(Modifier::UNDERLINED)
                    .underline_color(underline_color)
            );
        }
    }
}
```

Note: `Modifier::UNDERCURLED` requires terminal support (Kitty/WezTerm/iTerm2). Fall back to `UNDERLINED` for broader compatibility. Can detect terminal capabilities later.

**Step 2: Build, test, commit**

```
feat(tui): add diagnostic underlines in editor
```

---

### Task 1.3: Show diagnostic message on cursor line

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs` (render_editor, after main loop)

**Step 1: Show inline diagnostic for current cursor line**

After the main line loop, if the cursor line has diagnostics, show the first one in a dimmed line below the editor content:

```rust
// After Paragraph render, show diagnostic message for cursor line
if let Some(diag) = self.diagnostics.most_severe_on_line(self.buffer.cursor().0) {
    let icon = match diag.severity {
        DiagnosticSeverity::Error => "● ",
        DiagnosticSeverity::Warning => "▲ ",
        DiagnosticSeverity::Hint => "◆ ",
    };
    let color = match diag.severity {
        DiagnosticSeverity::Error => Color::Rgb(243, 139, 168),
        DiagnosticSeverity::Warning => Color::Rgb(249, 226, 175),
        DiagnosticSeverity::Hint => Color::Rgb(137, 180, 250),
    };
    let msg = format!("{}{}: {}", icon, diag.code, diag.message);
    let diag_line = Line::from(Span::styled(msg, Style::default().fg(color)));
    // Render in the status area (already split at line 2202-2213)
    frame.render_widget(Paragraph::new(diag_line), status_area);
}
```

**Step 2: Build, test, commit**

```
feat(tui): show diagnostic message on cursor line
```

---

## CHECKPOINT 1: Diagnostic rendering

Run: `cargo build --features tui && cargo test --lib -- tui`

Verify:
- [ ] Gutter shows ● (red) / ▲ (yellow) / ◆ (blue) for diagnostic lines
- [ ] Underlines appear on error spans
- [ ] Cursor line diagnostic message shown in editor status area
- [ ] Non-diagnostic lines show clean 2-char gutter placeholder

---

## Phase 2: Wire Tree-Sitter Highlighter

### Task 2.1: Replace YamlHighlight with TreeSitterHighlighter

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs` (YamlEditorPanel struct + render_editor)
- Read: `tools/nika/src/tui/highlight/treesitter.rs`

**Step 1: Add TreeSitterHighlighter field to YamlEditorPanel**

In the YamlEditorPanel struct (line 1562), add:
```rust
#[cfg(feature = "tui")]
ts_highlighter: Option<crate::tui::highlight::treesitter::TreeSitterHighlighter>,
```

Initialize in `new()`:
```rust
ts_highlighter: crate::tui::highlight::treesitter::TreeSitterHighlighter::new().ok(),
```

**Step 2: Use tree-sitter for highlighting in render_editor**

Replace the `YamlHighlight::highlight_line(line, base_style)` call (around line 2325/2338/2344) with:

```rust
let highlighted_spans = if let Some(ref mut ts) = self.ts_highlighter {
    // Use tree-sitter for the full document, get spans for this line
    let content = self.buffer.content();
    let all_lines = ts.highlight(&content);
    if i < all_lines.len() {
        all_lines[i].spans.clone()
    } else {
        vec![Span::styled(line.to_string(), base_style)]
    }
} else {
    // Fallback to regex-based highlighting
    YamlHighlight::highlight_line(line, base_style)
};
```

Note: This is inefficient (re-highlighting full doc per line). Optimize in Task 2.2.

**Step 3: Build, test, commit**

```
feat(tui): wire tree-sitter highlighter to editor
```

---

### Task 2.2: Optimize tree-sitter — cache per-frame, highlight once

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs`

**Step 1: Cache highlighted lines**

Instead of calling `highlight()` inside the per-line loop, compute it ONCE before the loop:

```rust
// Before the main loop (line ~2270):
let ts_lines: Option<Vec<Line>> = self.ts_highlighter.as_ref().map(|ts| {
    ts.highlight(&self.buffer.content())
});

// Inside the loop, for each line i:
let highlighted_spans = if let Some(ref lines) = ts_lines {
    if i < lines.len() {
        lines[i].spans.clone()
    } else {
        vec![Span::styled(line.to_string(), base_style)]
    }
} else {
    YamlHighlight::highlight_line(line, base_style)
};
```

**Step 2: Use incremental parsing on edits**

When `insert_char`, `insert_newline`, `backspace`, `delete` are called, notify the tree-sitter parser of the edit range for incremental re-parsing:

```rust
// After any text mutation:
if let Some(ref mut ts) = self.ts_highlighter {
    ts.highlight_incremental(&self.buffer.content(), edit_start, old_end, new_end);
}
```

**Step 3: Build, test, commit**

```
perf(tui): cache tree-sitter highlights per frame
```

---

### Task 2.3: Update tree-sitter theme to Catppuccin Mocha

**Files:**
- Modify: `tools/nika/src/tui/highlight/theme.rs`

**Step 1: Replace Solarized colors with Catppuccin Mocha**

```rust
// Replace all hardcoded Solarized colors:
pub const KEYWORD: Color = Color::Rgb(203, 166, 247);   // Mauve (was Solarized GREEN)
pub const VERB: Color = Color::Rgb(203, 166, 247);       // Mauve
pub const STRING: Color = Color::Rgb(166, 227, 161);     // Green
pub const NUMBER: Color = Color::Rgb(250, 179, 135);     // Peach
pub const BOOLEAN: Color = Color::Rgb(250, 179, 135);    // Peach
pub const COMMENT: Color = Color::Rgb(108, 112, 134);    // Overlay0
pub const PROPERTY: Color = Color::Rgb(137, 180, 250);   // Blue (hacking)
pub const TEMPLATE: Color = Color::Rgb(148, 226, 213);   // Teal
pub const MCP_SERVER: Color = Color::Rgb(116, 199, 236); // Sapphire
pub const TASK_ID: Color = Color::Rgb(249, 226, 175);    // Yellow
pub const ERROR: Color = Color::Rgb(243, 139, 168);      // Red
```

**Step 2: Build, test, commit**

```
style(tui): Catppuccin Mocha colors for tree-sitter theme
```

---

## CHECKPOINT 2: Syntax highlighting

Run: `cargo build --features tui && cargo test --lib -- tui`

Verify:
- [ ] Tree-sitter highlights Nika verbs (infer/exec/fetch/invoke/agent) in Mauve
- [ ] Templates `{{with.x}}` highlighted in Teal
- [ ] MCP server names highlighted in Sapphire
- [ ] Task IDs highlighted in Yellow
- [ ] Comments in Overlay0 (dimmed)
- [ ] Fallback to regex if tree-sitter fails

---

## Phase 3: Navigation Keys

### Task 3.1: Add Home/End/PageUp/PageDown

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs` (TextBuffer + handle_insert_mode)

**Step 1: Add methods to TextBuffer**

```rust
// After cursor_right() (line ~1116):

pub fn cursor_home(&mut self) {
    self.cursor_col = 0;
}

pub fn cursor_end(&mut self) {
    self.cursor_col = self.lines[self.cursor_row].len();
}

pub fn page_up(&mut self) {
    let page = self.visible_height.get().saturating_sub(2);
    self.cursor_row = self.cursor_row.saturating_sub(page);
    self.clamp_cursor();
    self.ensure_cursor_visible();
}

pub fn page_down(&mut self) {
    let page = self.visible_height.get().saturating_sub(2);
    self.cursor_row = (self.cursor_row + page).min(self.lines.len().saturating_sub(1));
    self.clamp_cursor();
    self.ensure_cursor_visible();
}
```

**Step 2: Wire in handle_insert_mode**

In the key event match (around line 2050), add:
```rust
KeyCode::Home => { self.buffer.cursor_home(); }
KeyCode::End => { self.buffer.cursor_end(); }
KeyCode::PageUp => { self.buffer.page_up(); }
KeyCode::PageDown => { self.buffer.page_down(); }
```

Also add in handle_normal_mode for Normal mode.

**Step 3: Build, test, commit**

```
feat(tui): add Home/End/PageUp/PageDown navigation
```

---

### Task 3.2: Add word-wise movement (Ctrl+Left/Right)

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs` (TextBuffer)

**Step 1: Add word boundary detection**

```rust
pub fn cursor_word_left(&mut self) {
    let line = &self.lines[self.cursor_row];
    if self.cursor_col == 0 {
        // Wrap to end of previous line
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
        }
        return;
    }
    let chars: Vec<char> = line.chars().collect();
    let mut col = self.cursor_col.min(chars.len());
    // Skip whitespace
    while col > 0 && chars[col - 1].is_whitespace() { col -= 1; }
    // Skip word chars
    while col > 0 && !chars[col - 1].is_whitespace() { col -= 1; }
    self.cursor_col = col;
}

pub fn cursor_word_right(&mut self) {
    let line = &self.lines[self.cursor_row];
    let chars: Vec<char> = line.chars().collect();
    if self.cursor_col >= chars.len() {
        // Wrap to start of next line
        if self.cursor_row < self.lines.len() - 1 {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
        return;
    }
    let mut col = self.cursor_col;
    // Skip word chars
    while col < chars.len() && !chars[col].is_whitespace() { col += 1; }
    // Skip whitespace
    while col < chars.len() && chars[col].is_whitespace() { col += 1; }
    self.cursor_col = col;
}
```

**Step 2: Wire Ctrl+Left/Right in handle_insert_mode**

```rust
KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
    self.buffer.cursor_word_left();
}
KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
    self.buffer.cursor_word_right();
}
```

**Step 3: Build, test, commit**

```
feat(tui): add word-wise cursor movement (Ctrl+Left/Right)
```

---

### Task 3.3: Smart indent on Enter

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs` (TextBuffer::insert_newline)

**Step 1: Auto-indent matching previous line**

In `insert_newline()` (line 1138), after splitting the line, detect the indent of the current line and apply it to the new line:

```rust
pub fn insert_newline(&mut self) {
    let current_line = &self.lines[self.cursor_row];
    // Detect indent level of current line
    let indent: String = current_line.chars().take_while(|c| c.is_whitespace()).collect();
    // Check if line ends with ':' → add 2 more spaces
    let extra_indent = if current_line.trim_end().ends_with(':') { "  " } else { "" };

    let after = self.lines[self.cursor_row].split_off(self.cursor_col);
    let new_line = format!("{}{}{}", indent, extra_indent, after.trim_start());
    self.lines.insert(self.cursor_row + 1, new_line);
    self.cursor_row += 1;
    self.cursor_col = indent.len() + extra_indent.len();
    self.modified = true;
}
```

This gives YAML-aware indent: entering after `tasks:` indents 2 more spaces.

**Step 2: Build, test, commit**

```
feat(tui): smart YAML-aware indent on Enter
```

---

## CHECKPOINT 3: Navigation complete

Run: `cargo build --features tui && cargo test --lib -- tui`

Verify:
- [ ] Home → start of line, End → end of line
- [ ] PageUp/PageDown scroll by viewport height
- [ ] Ctrl+Left/Right move by word boundaries
- [ ] Enter auto-indents matching previous line
- [ ] Enter after `key:` adds 2 extra spaces

---

## Phase 4: In-Process LSP — Completion + Hover

### Task 4.1: Add LSP handler to YamlEditorPanel

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs` (YamlEditorPanel struct)

**Step 1: Add handler field**

```rust
pub struct YamlEditorPanel {
    // ... existing fields ...

    /// In-process LSP handler for completion/hover (feature-gated)
    #[cfg(feature = "lsp")]
    lsp_handler: nika_lsp_core::handler::DefaultHandler,
}
```

Initialize in `new()`:
```rust
#[cfg(feature = "lsp")]
lsp_handler: nika_lsp_core::handler::DefaultHandler::new(),
```

**Step 2: Add completion state**

```rust
/// Completion popup state
pub struct CompletionState {
    pub visible: bool,
    pub items: Vec<CompletionEntry>,
    pub selected: usize,
    pub trigger_col: usize, // Column where completion was triggered
}

pub struct CompletionEntry {
    pub label: String,
    pub kind: String, // "verb", "field", "model", "task", etc.
    pub detail: Option<String>,
    pub insert_text: String,
}
```

Add `completion: CompletionState` to YamlEditorPanel.

**Step 3: Build, commit**

```
feat(tui): add LSP handler + completion state to editor
```

---

### Task 4.2: Trigger completion on `:` and Ctrl+Space

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs` (handle_insert_mode)

**Step 1: Detect trigger characters**

After `insert_char()`, check if we should trigger completion:

```rust
KeyCode::Char(c) => {
    self.editor.buffer.insert_char(c);
    // ... existing logic ...

    // Trigger completion on trigger characters
    #[cfg(feature = "lsp")]
    if matches!(c, ':' | '.' | '$' | '{' | ' ') {
        self.editor.trigger_completion();
    }
}
```

Also add Ctrl+Space as explicit trigger:
```rust
KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    #[cfg(feature = "lsp")]
    self.editor.trigger_completion();
}
```

**Step 2: Implement trigger_completion()**

```rust
#[cfg(feature = "lsp")]
fn trigger_completion(&mut self) {
    use nika_lsp_core::analysis::context::detect_context;
    use nika_lsp_core::handler::LspHandler;

    let content = self.buffer.content();
    let offset = self.buffer.linear_cursor_offset() as u32;
    let context = detect_context(&content, offset, None);
    let items = self.lsp_handler.completion(&content, offset, &context);

    self.completion.items = items.iter().map(|item| CompletionEntry {
        label: item.label.clone(),
        kind: format!("{:?}", item.kind.unwrap_or(ls_types::CompletionItemKind::TEXT)),
        detail: item.detail.clone(),
        insert_text: item.insert_text.clone().unwrap_or_else(|| item.label.clone()),
    }).collect();
    self.completion.selected = 0;
    self.completion.trigger_col = self.buffer.cursor().1;
    self.completion.visible = !self.completion.items.is_empty();
}
```

**Step 3: Build, commit**

```
feat(tui): trigger LSP completions on : and Ctrl+Space
```

---

### Task 4.3: Render completion popup overlay

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs` (render_editor, after main content)

**Step 1: Render completion dropdown**

After the editor content renders, if completion is visible, render an overlay:

```rust
if self.completion.visible && !self.completion.items.is_empty() {
    let max_items = 8.min(self.completion.items.len());
    let max_width = self.completion.items.iter()
        .map(|i| i.label.len() + i.kind.len() + 3)
        .max().unwrap_or(20).min(40) as u16;

    // Position below cursor
    let cursor_row = self.buffer.cursor().0 - self.buffer.scroll_offset();
    let cursor_col = self.buffer.cursor().1;
    let gutter_width = 8u16;
    let popup_x = content_area.x + gutter_width + cursor_col as u16;
    let popup_y = content_area.y + cursor_row as u16 + 1;

    let popup_area = Rect::new(
        popup_x.min(content_area.right().saturating_sub(max_width)),
        popup_y.min(content_area.bottom().saturating_sub(max_items as u16 + 2)),
        max_width + 2,
        max_items as u16 + 2,
    );

    // Clear area + render bordered list
    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(69, 71, 90))) // Surface1
        .style(Style::default().bg(Color::Rgb(24, 24, 37)));       // Mantle

    let items: Vec<ListItem> = self.completion.items.iter().enumerate()
        .take(max_items)
        .map(|(idx, item)| {
            let style = if idx == self.completion.selected {
                Style::default()
                    .bg(Color::Rgb(69, 71, 90))  // Surface1
                    .fg(Color::Rgb(205, 214, 244)) // Text
            } else {
                Style::default().fg(Color::Rgb(186, 194, 222)) // Subtext1
            };
            ListItem::new(Line::from(vec![
                Span::styled(&item.label, style),
                Span::styled(format!(" {}", item.kind), style.fg(Color::Rgb(108, 112, 134))),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, popup_area);
}
```

**Step 2: Handle completion navigation keys**

In handle_insert_mode, before normal key handling:
```rust
if self.editor.completion.visible {
    match key.code {
        KeyCode::Down => { self.editor.completion.selected = (self.editor.completion.selected + 1).min(self.editor.completion.items.len() - 1); return ViewAction::None; }
        KeyCode::Up => { self.editor.completion.selected = self.editor.completion.selected.saturating_sub(1); return ViewAction::None; }
        KeyCode::Enter | KeyCode::Tab => { self.editor.accept_completion(); return ViewAction::None; }
        KeyCode::Esc => { self.editor.completion.visible = false; return ViewAction::None; }
        _ => { self.editor.completion.visible = false; } // Dismiss on any other key
    }
}
```

**Step 3: Implement accept_completion()**

```rust
fn accept_completion(&mut self) {
    if let Some(item) = self.completion.items.get(self.completion.selected) {
        let insert = item.insert_text.clone();
        // Delete from trigger_col to cursor
        let cursor_col = self.buffer.cursor().1;
        for _ in self.completion.trigger_col..cursor_col {
            self.buffer.backspace();
        }
        // Insert the completion text
        for c in insert.chars() {
            self.buffer.insert_char(c);
        }
    }
    self.completion.visible = false;
}
```

**Step 4: Build, test, commit**

```
feat(tui): render completion popup with LSP results
```

---

### Task 4.4: Add hover tooltip on Ctrl+H or K key (Normal mode)

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs`

**Step 1: Add hover state**

```rust
pub struct HoverState {
    pub visible: bool,
    pub content: String,
    pub line: usize,
    pub col: usize,
}
```

**Step 2: Trigger hover in Normal mode**

```rust
// In handle_normal_mode, add:
KeyCode::Char('K') => {
    #[cfg(feature = "lsp")]
    {
        use nika_lsp_core::analysis::context::detect_context;
        use nika_lsp_core::handler::LspHandler;

        let content = self.editor.buffer.content();
        let offset = self.editor.buffer.linear_cursor_offset() as u32;
        let context = detect_context(&content, offset, None);

        if let Some(hover) = self.editor.lsp_handler.hover(&content, offset, &context) {
            self.editor.hover = HoverState {
                visible: true,
                content: hover.contents,
                line: self.editor.buffer.cursor().0,
                col: self.editor.buffer.cursor().1,
            };
        }
    }
}
```

**Step 3: Render hover tooltip**

After completion popup rendering:
```rust
if self.hover.visible {
    // Render a bordered popup with markdown-lite content
    let lines: Vec<&str> = self.hover.content.lines().take(10).collect();
    let height = lines.len() as u16 + 2;
    let width = lines.iter().map(|l| l.len()).max().unwrap_or(20).min(60) as u16 + 2;

    // Position above cursor
    let popup_area = Rect::new(
        content_area.x + 8 + self.hover.col as u16,
        content_area.y + (self.hover.line.saturating_sub(self.buffer.scroll_offset())) as u16 - height,
        width, height,
    );

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(" Hover ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(180, 190, 254))) // Lavender
        .style(Style::default().bg(Color::Rgb(24, 24, 37)));          // Mantle

    let text: Vec<Line> = lines.iter().map(|l| {
        Line::from(Span::styled(*l, Style::default().fg(Color::Rgb(205, 214, 244))))
    }).collect();

    frame.render_widget(Paragraph::new(text).block(block).wrap(Wrap { trim: false }), popup_area);
}
```

**Step 4: Dismiss hover on any keypress**

At the top of all key handlers:
```rust
self.editor.hover.visible = false;
```

**Step 5: Build, test, commit**

```
feat(tui): add hover documentation tooltip (K key in Normal mode)
```

---

## CHECKPOINT 4: LSP integration

Run: `cargo build --features tui,lsp && cargo test --lib -- tui`

Verify:
- [ ] Typing `:` after a task field triggers completion popup
- [ ] Ctrl+Space opens completion anywhere
- [ ] Up/Down navigate completion items
- [ ] Enter/Tab accepts completion
- [ ] Esc dismisses completion
- [ ] K in Normal mode shows hover documentation
- [ ] Hover shows verb docs, field descriptions, task references
- [ ] Completion works for: verbs, task fields, model names, with bindings

---

## Phase 5: Go-to-Definition + Code Actions

### Task 5.1: Go-to-definition (gd in Normal mode)

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs`

**Step 1: Add gd keybinding (which-key group)**

In handle_normal_mode, within the 'g' which-key prefix handler:
```rust
// After existing 'gg' (go to top), add:
KeyCode::Char('d') => {
    #[cfg(feature = "lsp")]
    {
        let content = self.editor.buffer.content();
        let offset = self.editor.buffer.linear_cursor_offset() as u32;
        let context = detect_context(&content, offset, None);

        if let Some(def) = self.editor.lsp_handler.definition(&content, offset, &context) {
            // Jump to definition line/col
            self.editor.buffer.set_cursor_position(def.target_line as usize, def.target_col as usize);
            self.editor.buffer.ensure_cursor_visible();
        }
    }
}
```

**Step 2: Build, test, commit**

```
feat(tui): add go-to-definition (gd) via LSP
```

---

### Task 5.2: Code actions menu (Ctrl+.)

**Files:**
- Modify: `tools/nika/src/tui/views/studio.rs`

**Step 1: Add code action state + trigger**

```rust
pub struct CodeActionState {
    pub visible: bool,
    pub actions: Vec<CodeActionEntry>,
    pub selected: usize,
}
```

Trigger on Ctrl+. :
```rust
KeyCode::Char('.') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    let content = self.editor.buffer.content();
    let offset = self.editor.buffer.linear_cursor_offset() as u32;
    let actions = self.editor.lsp_handler.code_actions(&content, offset, offset + 1);
    self.editor.code_actions = CodeActionState {
        visible: !actions.is_empty(),
        actions,
        selected: 0,
    };
}
```

**Step 2: Render + apply actions (similar pattern to completion popup)**

**Step 3: Build, test, commit**

```
feat(tui): add code actions menu (Ctrl+.)
```

---

## CHECKPOINT 5: Full IDE experience

Run: `cargo build --features tui,lsp && cargo test --lib -- tui`

Verify:
- [ ] `gd` jumps to task definition
- [ ] Ctrl+. shows code actions (add schema, expand shorthand, etc.)
- [ ] All 50+ completion contexts work
- [ ] Hover docs show for all verbs and fields
- [ ] Diagnostics show in gutter + underline + message
- [ ] Tree-sitter highlighting active
- [ ] Navigation: Home/End/PgUp/PgDn/Ctrl+Arrow all work
- [ ] Smart indent on Enter works for YAML

---

## Summary

| Phase | Tasks | Impact |
|-------|-------|--------|
| **1. Diagnostics** | 3 tasks | Gutter icons + underlines + cursor message |
| **2. Tree-sitter** | 3 tasks | Semantic highlighting (verbs/templates/MCP) |
| **3. Navigation** | 3 tasks | Home/End/PgUp/PgDn/word-move/smart-indent |
| **4. LSP Core** | 4 tasks | Completion popup + hover tooltip |
| **5. Advanced** | 2 tasks | Go-to-def + code actions |
| **Total** | 15 tasks | Full IDE editor in TUI |

Estimated time: 3-4 hours focused implementation.
