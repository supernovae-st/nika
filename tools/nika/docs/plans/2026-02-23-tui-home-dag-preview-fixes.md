# TUI Home View DAG Preview Fixes

**Date:** 2026-02-23
**Status:** In Progress
**Related Files:** `src/tui/views/home.rs`, `src/tui/widgets/dag_*.rs`

---

## Overview

The Home view DAG preview widget is not rendering correctly. This plan addresses:
1. Critical bugs identified by audit agents
2. New feature: Toggle between DAG and YAML views
3. Verb-colored YAML syntax highlighting
4. Performance optimizations

---

## Phase 1: Critical Bug Fixes

### Bug 1.1: Node Height Mismatch (CRITICAL)

**File:** `src/tui/widgets/dag_layout.rs:355`

**Problem:**
```rust
// Current (WRONG)
let node_height = if config.expanded { 3 } else { 1 };  // 1 is too small!
```

NodeBox requires minimum height of 3 lines:
- Line 1: Top border (`┌────────────┐`)
- Line 2: Content row
- Line 3: Bottom border (`└────────────┘`)

**Fix:**
```rust
// Correct heights
let node_height = if config.expanded { 5 } else { 3 };  // Minimal = 3, Expanded = 5
```

---

### Bug 1.2: Coordinate Offset Missing (CRITICAL)

**File:** `src/tui/widgets/dag_ascii.rs`

**Problem:** `render_nodes()` and `render_edges()` don't offset coordinates by `area.x` and `area.y`.

When the DAG preview area starts at `(x=50, y=2)`, nodes render at `(0, 0)` instead.

**Affected functions:**
- `render_nodes()` (lines 248-276)
- `render_edges()` (lines 185-243)

**Fix for render_nodes():**
```rust
fn render_nodes(&self, buf: &mut Buffer, area: Rect, layout: &DagLayout) {
    for node_data in self.nodes {
        if let Some(pos) = layout.get(&node_data.id) {
            // Apply both scroll offset AND area origin offset
            let x = area.x + pos.x.saturating_sub(self.scroll.0);
            let y = area.y + pos.y.saturating_sub(self.scroll.1);

            // Bounds check
            if x < area.x || x >= area.x + area.width ||
               y < area.y || y >= area.y + area.height {
                continue;
            }

            // Render node at correct position
            // ...
        }
    }
}
```

**Fix for render_edges():**
```rust
fn render_edges(&self, buf: &mut Buffer, area: Rect, layout: &DagLayout) {
    for (from_id, to_id) in &self.edges {
        if let (Some(from_pos), Some(to_pos)) = (layout.get(from_id), layout.get(to_id)) {
            // Apply area origin offset
            let x1 = area.x + from_pos.x.saturating_sub(self.scroll.0);
            let y1 = area.y + from_pos.y.saturating_sub(self.scroll.1);
            let x2 = area.x + to_pos.x.saturating_sub(self.scroll.0);
            let y2 = area.y + to_pos.y.saturating_sub(self.scroll.1);

            // Draw edge with correct coordinates
            // ...
        }
    }
}
```

---

## Phase 2: Preview Mode Toggle

### Feature 2.1: Add PreviewMode Enum

**File:** `src/tui/views/home.rs`

```rust
/// Preview mode for selected file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewMode {
    #[default]
    Dag,   // DAG visualization (default)
    Yaml,  // Raw YAML text with verb highlighting
}

pub struct HomeView {
    // ... existing fields ...

    /// Current preview mode (DAG or YAML)
    pub preview_mode: PreviewMode,
}
```

### Feature 2.2: Toggle Key Handler

**Key binding:** `D` to toggle between DAG and YAML preview

```rust
// In handle_key_event()
KeyCode::Char('d') | KeyCode::Char('D') => {
    self.preview_mode = match self.preview_mode {
        PreviewMode::Dag => PreviewMode::Yaml,
        PreviewMode::Yaml => PreviewMode::Dag,
    };
    true
}
```

### Feature 2.3: Render Dispatch

```rust
fn render_preview(&mut self, area: Rect, buf: &mut Buffer) {
    match self.preview_mode {
        PreviewMode::Dag => self.render_dag_preview(area, buf),
        PreviewMode::Yaml => self.render_yaml_preview(area, buf),
    }
}
```

---

## Phase 3: Verb-Colored YAML Syntax Highlighting

### Feature 3.1: Define Verb Colors

**File:** `src/tui/views/home.rs`

```rust
/// Get verb color for YAML line highlighting
fn get_verb_color(line: &str) -> Option<Color> {
    let trimmed = line.trim();

    // Check for verb declarations
    if trimmed.starts_with("infer:") {
        Some(Color::Cyan)        // ⚡ Infer - cyan
    } else if trimmed.starts_with("exec:") {
        Some(Color::Yellow)      // 📟 Exec - yellow
    } else if trimmed.starts_with("fetch:") {
        Some(Color::Blue)        // 🛰️ Fetch - blue
    } else if trimmed.starts_with("invoke:") {
        Some(Color::Magenta)     // 🔌 Invoke - magenta
    } else if trimmed.starts_with("agent:") {
        Some(Color::Red)         // 🐔 Agent - red
    } else {
        None
    }
}
```

### Feature 3.2: Render YAML with Colors

```rust
fn render_yaml_preview(&self, area: Rect, buf: &mut Buffer) {
    let content = match &self.preview_content {
        Some(c) => c,
        None => return,
    };

    // Split into lines
    let lines: Vec<&str> = content.lines().collect();

    // Track current verb context for indented lines
    let mut current_verb_color: Option<Color> = None;

    for (i, line) in lines.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }

        let y = area.y + i as u16;

        // Check if line starts a new verb block
        if let Some(color) = Self::get_verb_color(line) {
            current_verb_color = Some(color);
        } else if !line.starts_with(' ') && !line.starts_with('\t') {
            // Non-indented line resets verb context
            current_verb_color = None;
        }

        // Apply color to line
        let style = match current_verb_color {
            Some(color) => Style::default().fg(color),
            None => Style::default().fg(Color::Gray),
        };

        // Render line
        let span = Span::styled(line.to_string(), style);
        buf.set_span(area.x, y, &span, area.width);
    }
}
```

### Color Mapping (Matching DAG verb icons)

| Verb | Icon | DAG Color | YAML Color |
|------|------|-----------|------------|
| `infer:` | ⚡ | Cyan | `Color::Cyan` |
| `exec:` | 📟 | Yellow | `Color::Yellow` |
| `fetch:` | 🛰️ | Blue | `Color::Blue` |
| `invoke:` | 🔌 | Magenta | `Color::Magenta` |
| `agent:` | 🐔 | Red | `Color::Red` |

---

## Phase 4: Performance Optimizations

### Issue 4.1: String Allocations in Render Loops

**Files:**
- `src/tui/widgets/dag_node_box.rs:563-568`
- `src/tui/widgets/dag_edge.rs:259,293,332`

**Problem:** `to_string()` called on border chars in tight loops

**Fix:** Use static `&str` slices instead:
```rust
// Before (allocates)
let corner = "┌".to_string();

// After (zero allocation)
const CORNER_TL: &str = "┌";
const CORNER_TR: &str = "┐";
const CORNER_BL: &str = "└";
const CORNER_BR: &str = "┘";
const EDGE_H: &str = "─";
const EDGE_V: &str = "│";
```

### Issue 4.2: HashMap Allocation per Frame

**Problem:** Creating new HashMaps for layout every frame

**Fix:** Cache layout computation:
```rust
pub struct HomeView {
    // ... existing fields ...

    /// Cached DAG layout (avoid recomputing every frame)
    cached_layout: RefCell<Option<DagLayout>>,
    /// Hash of workflow for cache invalidation
    cached_workflow_hash: Cell<u64>,
}
```

### Issue 4.3: format!() in Render Loops

**File:** `src/tui/widgets/dag_node_box.rs:620-628, 677-685`

**Fix:** Pre-compute formatted strings in update, not render

---

## Phase 5: UI Improvements

### Status Bar Indicator

Show current preview mode in status bar:

```
[D]AG | YAML     ← indicates current mode and toggle key
```

### Header Label

Add preview area header showing mode:

```
┌─ DAG Preview ──────────────────┐   ← when in DAG mode
│                                │
│     ⚡ → 📟 → 🛰️               │
│                                │
└────────────────────────────────┘

┌─ YAML (verb-colored) ──────────┐   ← when in YAML mode
│ tasks:                         │
│   - id: generate               │
│     infer: "Generate headline" │ ← cyan
│   - id: build                  │
│     exec: "npm run build"      │ ← yellow
└────────────────────────────────┘
```

---

## Implementation Order

1. **[P1]** Fix node height (dag_layout.rs) - 5 min
2. **[P1]** Fix coordinate offset (dag_ascii.rs) - 15 min
3. **[P2]** Add PreviewMode enum - 5 min
4. **[P2]** Add toggle key handler - 5 min
5. **[P3]** Add verb color mapping - 5 min
6. **[P3]** Implement render_yaml_preview - 20 min
7. **[P4]** Replace to_string() with static str - 15 min
8. **[P4]** Cache layout computation - 10 min
9. **[P5]** Add status bar mode indicator - 5 min
10. **Build & Test** - 10 min
11. **Commit** - 5 min

**Total Estimate:** ~100 minutes

---

## Testing Checklist

- [ ] DAG renders correctly in preview area
- [ ] Nodes are positioned at correct coordinates
- [ ] Edges connect nodes properly
- [ ] `D` key toggles between DAG and YAML views
- [ ] YAML view shows verb-colored lines
- [ ] Indented lines inherit verb color
- [ ] Status bar shows current mode
- [ ] No performance regression (measure with `cargo bench`)
- [ ] All 2,323 tests pass

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/tui/widgets/dag_layout.rs` | Fix node_height (1→3) |
| `src/tui/widgets/dag_ascii.rs` | Add area offset to coordinates |
| `src/tui/views/home.rs` | Add PreviewMode, toggle, YAML render |
| `src/tui/widgets/dag_node_box.rs` | Static strings for borders |
| `src/tui/widgets/dag_edge.rs` | Static strings for edge chars |
