# DAG ASCII Visualizer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Create an intelligent ASCII DAG visualizer for the browser view with proper visual encoding, Sugiyama layout, and expandable node boxes.

**Architecture:** Integrated module in `tui/widgets/` with 4 new files (layout, node_box, edge, main widget) plus theme updates for verb colors.

**Tech Stack:** Rust, ratatui, existing TUI infrastructure

---

## Visual Encoding System

### Verb Colors (RGB + HEX)

| Verb   | Icon | Color   | RGB           | HEX     | Semantic            |
|--------|------|---------|---------------|---------|---------------------|
| infer  | 🧠   | Violet  | (139,92,246)  | #8B5CF6 | AI reasoning        |
| exec   | ⚡   | Amber   | (245,158,11)  | #F59E0B | Shell execution     |
| fetch  | 🌐   | Cyan    | (6,182,212)   | #06B6D4 | Network/HTTP        |
| invoke | 🔧   | Emerald | (16,185,129)  | #10B981 | MCP tool call       |
| agent  | 🤖   | Rose    | (244,63,94)   | #F43F5E | Autonomous loop     |

### Border Styles (by Status)

| Status  | Style | Description                    |
|---------|-------|--------------------------------|
| pending | ┄┄┄   | Dashed, 50% opacity color      |
| running | ━━━   | Bold, pulsing, full color      |
| success | ═══   | Double line, full color + ✓    |
| failed  | ╳╳╳   | Crossed, red override + ✗      |
| paused  | ┈┈┈   | Dotted, cyan accent            |

---

## Task 1: VerbTheme in theme.rs

**Files:**
- Modify: `src/tui/theme.rs`

**Step 1: Add VerbColor enum and colors**

```rust
/// Verb-specific colors for DAG visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbColor {
    Infer,   // Violet #8B5CF6
    Exec,    // Amber #F59E0B
    Fetch,   // Cyan #06B6D4
    Invoke,  // Emerald #10B981
    Agent,   // Rose #F43F5E
}

impl VerbColor {
    /// Get the RGB color for this verb
    pub fn rgb(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(139, 92, 246),   // Violet
            Self::Exec => Color::Rgb(245, 158, 11),    // Amber
            Self::Fetch => Color::Rgb(6, 182, 212),    // Cyan
            Self::Invoke => Color::Rgb(16, 185, 129),  // Emerald
            Self::Agent => Color::Rgb(244, 63, 94),    // Rose
        }
    }

    /// Get muted version (50% opacity simulation)
    pub fn muted(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(97, 64, 171),
            Self::Exec => Color::Rgb(171, 110, 8),
            Self::Fetch => Color::Rgb(4, 127, 148),
            Self::Invoke => Color::Rgb(11, 129, 90),
            Self::Agent => Color::Rgb(170, 44, 66),
        }
    }

    /// Get icon for this verb
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Infer => "🧠",
            Self::Exec => "⚡",
            Self::Fetch => "🌐",
            Self::Invoke => "🔧",
            Self::Agent => "🤖",
        }
    }

    /// Parse from verb name string
    pub fn from_verb(verb: &str) -> Self {
        match verb.to_lowercase().as_str() {
            "infer" => Self::Infer,
            "exec" => Self::Exec,
            "fetch" => Self::Fetch,
            "invoke" => Self::Invoke,
            "agent" => Self::Agent,
            _ => Self::Infer, // default
        }
    }
}
```

**Step 2: Add to Theme struct**

```rust
impl Theme {
    /// Get verb color
    pub fn verb_color(&self, verb: VerbColor) -> Color {
        verb.rgb()
    }

    /// Get verb color muted
    pub fn verb_color_muted(&self, verb: VerbColor) -> Color {
        verb.muted()
    }
}
```

**Step 3: Run tests**

```bash
cargo test --features tui theme
```

**Step 4: Commit**

```bash
git add src/tui/theme.rs
git commit -m "feat(tui): add VerbColor enum with 5 verb colors

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: dag_layout.rs - Sugiyama Layout Algorithm

**Files:**
- Create: `src/tui/widgets/dag_layout.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create layout structs**

```rust
//! DAG Layout Algorithm (Sugiyama-style)
//!
//! Computes node positions for ASCII DAG visualization using a
//! layer-based approach with crossing minimization.

use std::collections::{HashMap, HashSet};

/// Position of a node in the DAG layout
#[derive(Debug, Clone, Copy, Default)]
pub struct NodePosition {
    /// Layer (vertical position, 0 = top)
    pub layer: usize,
    /// Order within layer (horizontal position)
    pub order: usize,
    /// X coordinate in characters
    pub x: u16,
    /// Y coordinate in characters
    pub y: u16,
    /// Width of the node box
    pub width: u16,
    /// Height of the node box
    pub height: u16,
}

/// Layout configuration
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Minimum horizontal spacing between nodes
    pub h_spacing: u16,
    /// Minimum vertical spacing between layers
    pub v_spacing: u16,
    /// Maximum width for node boxes
    pub max_node_width: u16,
    /// Expanded mode (larger boxes)
    pub expanded: bool,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            h_spacing: 4,
            v_spacing: 3,
            max_node_width: 50,
            expanded: false,
        }
    }
}

/// DAG layout calculator
pub struct DagLayout {
    /// Node positions by ID
    positions: HashMap<String, NodePosition>,
    /// Total width needed
    pub total_width: u16,
    /// Total height needed
    pub total_height: u16,
    /// Layers (list of node IDs per layer)
    layers: Vec<Vec<String>>,
}
```

**Step 2: Implement Sugiyama algorithm**

```rust
impl DagLayout {
    /// Compute layout for nodes with dependencies
    pub fn compute(
        nodes: &[(String, Vec<String>)], // (id, dependencies)
        config: &LayoutConfig,
        node_widths: &HashMap<String, u16>,
    ) -> Self {
        let mut layout = Self {
            positions: HashMap::new(),
            total_width: 0,
            total_height: 0,
            layers: Vec::new(),
        };

        if nodes.is_empty() {
            return layout;
        }

        // Step 1: Assign layers via topological sort
        layout.assign_layers(nodes);

        // Step 2: Order nodes within layers to minimize crossings
        layout.order_within_layers(nodes);

        // Step 3: Compute X/Y positions
        layout.compute_positions(config, node_widths);

        layout
    }

    /// Get position for a node
    pub fn get(&self, id: &str) -> Option<&NodePosition> {
        self.positions.get(id)
    }

    /// Get layer count
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Iterate layers
    pub fn layers(&self) -> &[Vec<String>] {
        &self.layers
    }

    // Private implementation methods...
    fn assign_layers(&mut self, nodes: &[(String, Vec<String>)]) {
        // Build dependency map
        let deps: HashMap<&str, &Vec<String>> = nodes
            .iter()
            .map(|(id, d)| (id.as_str(), d))
            .collect();

        let mut assigned: HashSet<String> = HashSet::new();
        let mut layers: Vec<Vec<String>> = Vec::new();

        // Assign nodes with no dependencies to layer 0
        let mut current_layer: Vec<String> = nodes
            .iter()
            .filter(|(_, d)| d.is_empty())
            .map(|(id, _)| id.clone())
            .collect();

        if current_layer.is_empty() && !nodes.is_empty() {
            // Fallback: first node to layer 0
            current_layer.push(nodes[0].0.clone());
        }

        while !current_layer.is_empty() {
            for id in &current_layer {
                assigned.insert(id.clone());
            }
            layers.push(current_layer.clone());

            // Find nodes whose dependencies are all assigned
            current_layer = nodes
                .iter()
                .filter(|(id, d)| {
                    !assigned.contains(id) && d.iter().all(|dep| assigned.contains(dep))
                })
                .map(|(id, _)| id.clone())
                .collect();
        }

        // Handle remaining nodes (cycles or disconnected)
        for (id, _) in nodes {
            if !assigned.contains(id) {
                if layers.is_empty() {
                    layers.push(Vec::new());
                }
                layers.last_mut().unwrap().push(id.clone());
            }
        }

        self.layers = layers;
    }

    fn order_within_layers(&mut self, _nodes: &[(String, Vec<String>)]) {
        // Simple ordering: keep original order for now
        // TODO: implement barycenter method for crossing minimization
    }

    fn compute_positions(
        &mut self,
        config: &LayoutConfig,
        node_widths: &HashMap<String, u16>,
    ) {
        let mut y: u16 = 0;
        let node_height = if config.expanded { 6 } else { 3 };

        for (layer_idx, layer) in self.layers.iter().enumerate() {
            let mut x: u16 = 0;

            for (order, id) in layer.iter().enumerate() {
                let width = node_widths.get(id).copied().unwrap_or(config.max_node_width);

                self.positions.insert(
                    id.clone(),
                    NodePosition {
                        layer: layer_idx,
                        order,
                        x,
                        y,
                        width,
                        height: node_height,
                    },
                );

                x += width + config.h_spacing;
                self.total_width = self.total_width.max(x);
            }

            y += node_height + config.v_spacing;
        }

        self.total_height = y;
    }
}
```

**Step 3: Add tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_dag() {
        let nodes = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["b".to_string()]),
        ];
        let config = LayoutConfig::default();
        let widths = HashMap::new();

        let layout = DagLayout::compute(&nodes, &config, &widths);

        assert_eq!(layout.layer_count(), 3);
        assert_eq!(layout.get("a").unwrap().layer, 0);
        assert_eq!(layout.get("b").unwrap().layer, 1);
        assert_eq!(layout.get("c").unwrap().layer, 2);
    }

    #[test]
    fn test_parallel_dag() {
        let nodes = vec![
            ("start".to_string(), vec![]),
            ("a".to_string(), vec!["start".to_string()]),
            ("b".to_string(), vec!["start".to_string()]),
            ("end".to_string(), vec!["a".to_string(), "b".to_string()]),
        ];
        let config = LayoutConfig::default();
        let widths = HashMap::new();

        let layout = DagLayout::compute(&nodes, &config, &widths);

        assert_eq!(layout.layer_count(), 3);
        assert_eq!(layout.get("start").unwrap().layer, 0);
        assert_eq!(layout.get("a").unwrap().layer, 1);
        assert_eq!(layout.get("b").unwrap().layer, 1);
        assert_eq!(layout.get("end").unwrap().layer, 2);
    }
}
```

**Step 4: Update mod.rs**

```rust
// In src/tui/widgets/mod.rs
mod dag_layout;
pub use dag_layout::{DagLayout, LayoutConfig, NodePosition};
```

**Step 5: Run tests**

```bash
cargo test --features tui dag_layout
```

**Step 6: Commit**

```bash
git add src/tui/widgets/dag_layout.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add Sugiyama-style DAG layout algorithm

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: dag_node_box.rs - Node Box Rendering

**Files:**
- Create: `src/tui/widgets/dag_node_box.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create node box struct**

```rust
//! DAG Node Box Rendering
//!
//! Renders individual task nodes with verb-colored borders,
//! icons, and expandable details.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::tui::theme::{TaskStatus, VerbColor};

/// Render mode for node boxes
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NodeBoxMode {
    #[default]
    Minimal,
    Expanded,
}

/// Node data for rendering
#[derive(Debug, Clone)]
pub struct NodeBoxData {
    /// Task ID
    pub id: String,
    /// Verb type
    pub verb: VerbColor,
    /// Current status
    pub status: TaskStatus,
    /// Estimated duration
    pub estimate: String,
    /// Prompt preview (for expanded mode)
    pub prompt_preview: Option<String>,
    /// Model name (for expanded mode)
    pub model: Option<String>,
    /// Use bindings (for expanded mode)
    pub use_bindings: Vec<(String, String)>,
    /// MCP servers (for invoke/agent)
    pub mcp_servers: Vec<String>,
    /// For_each items count
    pub for_each_count: Option<usize>,
    /// For_each item names (for mini-nodes)
    pub for_each_items: Vec<String>,
    /// Concurrency level
    pub concurrency: Option<usize>,
}

impl NodeBoxData {
    pub fn new(id: impl Into<String>, verb: VerbColor) -> Self {
        Self {
            id: id.into(),
            verb,
            status: TaskStatus::Pending,
            estimate: String::new(),
            prompt_preview: None,
            model: None,
            use_bindings: Vec::new(),
            mcp_servers: Vec::new(),
            for_each_count: None,
            for_each_items: Vec::new(),
            concurrency: None,
        }
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_estimate(mut self, estimate: impl Into<String>) -> Self {
        self.estimate = estimate.into();
        self
    }
}
```

**Step 2: Implement Widget for NodeBox**

```rust
/// Node box widget
pub struct NodeBox<'a> {
    data: &'a NodeBoxData,
    mode: NodeBoxMode,
    frame: u8,
}

impl<'a> NodeBox<'a> {
    pub fn new(data: &'a NodeBoxData) -> Self {
        Self {
            data,
            mode: NodeBoxMode::Minimal,
            frame: 0,
        }
    }

    pub fn mode(mut self, mode: NodeBoxMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Calculate required width for this node
    pub fn required_width(&self) -> u16 {
        let id_len = self.data.id.len();
        let estimate_len = self.data.estimate.len();
        let base = id_len + estimate_len + 10; // icon + spacing + borders

        match self.mode {
            NodeBoxMode::Minimal => base.min(50) as u16,
            NodeBoxMode::Expanded => (base + 20).min(70) as u16,
        }
    }

    /// Calculate required height for this node
    pub fn required_height(&self) -> u16 {
        match self.mode {
            NodeBoxMode::Minimal => {
                if self.data.for_each_count.is_some() { 4 } else { 3 }
            }
            NodeBoxMode::Expanded => {
                let mut height = 4; // border + header + separator + border
                if self.data.prompt_preview.is_some() { height += 1; }
                if self.data.model.is_some() { height += 1; }
                if !self.data.use_bindings.is_empty() { height += 2; }
                if !self.data.mcp_servers.is_empty() { height += 1; }
                if self.data.for_each_count.is_some() { height += 2; }
                height
            }
        }
    }

    fn border_chars(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self.data.status {
            TaskStatus::Pending => ("┏", "┓", "┗", "┛"),
            TaskStatus::Running => ("┏", "┓", "┗", "┛"),
            TaskStatus::Success => ("╔", "╗", "╚", "╝"),
            TaskStatus::Failed => ("╔", "╗", "╚", "╝"),
            TaskStatus::Paused => ("┌", "┐", "└", "┘"),
        }
    }

    fn h_border(&self) -> &'static str {
        match self.data.status {
            TaskStatus::Pending => "┄",
            TaskStatus::Running => "━",
            TaskStatus::Success => "═",
            TaskStatus::Failed => "═",
            TaskStatus::Paused => "┈",
        }
    }

    fn v_border(&self) -> &'static str {
        match self.data.status {
            TaskStatus::Pending => "┆",
            TaskStatus::Running => "┃",
            TaskStatus::Success => "║",
            TaskStatus::Failed => "║",
            TaskStatus::Paused => "┊",
        }
    }

    fn border_color(&self) -> Color {
        match self.data.status {
            TaskStatus::Failed => Color::Rgb(239, 68, 68), // Red override
            _ => self.data.verb.rgb(),
        }
    }
}

impl Widget for NodeBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            return;
        }

        let color = self.border_color();
        let style = Style::default().fg(color);
        let (tl, tr, bl, br) = self.border_chars();
        let h = self.h_border();
        let v = self.v_border();

        // Top border
        buf.set_string(area.x, area.y, tl, style);
        for x in (area.x + 1)..(area.x + area.width - 1) {
            buf.set_string(x, area.y, h, style);
        }
        buf.set_string(area.x + area.width - 1, area.y, tr, style);

        // Side borders and content
        for y in (area.y + 1)..(area.y + area.height - 1) {
            buf.set_string(area.x, y, v, style);
            buf.set_string(area.x + area.width - 1, y, v, style);
        }

        // Bottom border
        buf.set_string(area.x, area.y + area.height - 1, bl, style);
        for x in (area.x + 1)..(area.x + area.width - 1) {
            buf.set_string(x, area.y + area.height - 1, h, style);
        }
        buf.set_string(area.x + area.width - 1, area.y + area.height - 1, br, style);

        // Content: icon + id + estimate
        let content_x = area.x + 2;
        let content_y = area.y + 1;
        let icon = self.data.verb.icon();

        buf.set_string(content_x, content_y, icon, Style::default());
        buf.set_string(
            content_x + 3,
            content_y,
            &self.data.id,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        );

        // Estimate on the right
        if !self.data.estimate.is_empty() {
            let est_x = area.x + area.width - self.data.estimate.len() as u16 - 3;
            buf.set_string(
                est_x,
                content_y,
                &format!("⏱ {}", self.data.estimate),
                Style::default().fg(Color::DarkGray),
            );
        }

        // For_each mini-nodes
        if let Some(count) = self.data.for_each_count {
            if area.height >= 4 && !self.data.for_each_items.is_empty() {
                let items_y = area.y + 2;
                let mut x = content_x;
                for (i, item) in self.data.for_each_items.iter().take(5).enumerate() {
                    if x + 8 > area.x + area.width - 2 { break; }
                    let mini_box = format!("╭{}╮", "─".repeat(item.len().min(6)));
                    buf.set_string(x, items_y, &mini_box, Style::default().fg(Color::DarkGray));
                    buf.set_string(x, items_y + 1, &format!("│{:^width$}│", item, width = item.len().min(6)), Style::default().fg(Color::DarkGray));
                    x += item.len().min(6) as u16 + 4;
                }
                if count > 5 {
                    buf.set_string(x, items_y, &format!("+{}", count - 5), Style::default().fg(Color::DarkGray));
                }
            }
        }

        // Status badge
        let badge = match self.data.status {
            TaskStatus::Success => Some(("✓", Color::Rgb(34, 197, 94))),
            TaskStatus::Failed => Some(("✗", Color::Rgb(239, 68, 68))),
            TaskStatus::Running => Some(("●", color)),
            _ => None,
        };
        if let Some((badge_char, badge_color)) = badge {
            buf.set_string(
                area.x + area.width - 3,
                area.y,
                badge_char,
                Style::default().fg(badge_color),
            );
        }
    }
}
```

**Step 3: Add tests and export**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_box_creation() {
        let data = NodeBoxData::new("task1", VerbColor::Infer)
            .with_status(TaskStatus::Running)
            .with_estimate("~2-5s");

        assert_eq!(data.id, "task1");
        assert_eq!(data.verb, VerbColor::Infer);
        assert_eq!(data.status, TaskStatus::Running);
    }

    #[test]
    fn test_required_dimensions() {
        let data = NodeBoxData::new("generate_headline", VerbColor::Infer)
            .with_estimate("~2-5s");

        let widget = NodeBox::new(&data);
        assert!(widget.required_width() >= 20);
        assert_eq!(widget.required_height(), 3);

        let expanded = NodeBox::new(&data).mode(NodeBoxMode::Expanded);
        assert!(expanded.required_height() > 3);
    }
}
```

**Step 4: Update mod.rs, run tests, commit**

```bash
cargo test --features tui dag_node_box
git add src/tui/widgets/dag_node_box.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add NodeBox widget with verb colors and status borders

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: dag_edge.rs - Edge Rendering

**Files:**
- Create: `src/tui/widgets/dag_edge.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create edge structs**

```rust
//! DAG Edge Rendering
//!
//! Renders connections between nodes with binding labels
//! and grey data previews.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

/// Edge between two nodes
#[derive(Debug, Clone)]
pub struct DagEdge {
    /// Source node position (x, y of bottom center)
    pub from: (u16, u16),
    /// Target node position (x, y of top center)
    pub to: (u16, u16),
    /// Binding label (e.g., "{{use.data}}")
    pub binding: Option<String>,
    /// Data preview (shown in grey)
    pub preview: Option<String>,
    /// Is this edge active (data flowing)
    pub active: bool,
}

impl DagEdge {
    pub fn new(from: (u16, u16), to: (u16, u16)) -> Self {
        Self {
            from,
            to,
            binding: None,
            preview: None,
            active: false,
        }
    }

    pub fn with_binding(mut self, binding: impl Into<String>) -> Self {
        self.binding = Some(binding.into());
        self
    }

    pub fn with_preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self
    }

    /// Render this edge to the buffer
    pub fn render(&self, buf: &mut Buffer, area: Rect) {
        let color = if self.active {
            Color::Rgb(245, 158, 11) // Amber
        } else {
            Color::DarkGray
        };
        let style = Style::default().fg(color);
        let preview_style = Style::default().fg(Color::Rgb(100, 100, 100));

        // Simple vertical line for now
        let x = self.from.0;
        let start_y = self.from.1;
        let end_y = self.to.1;

        if x < area.x || x >= area.x + area.width {
            return;
        }

        // Draw vertical line
        for y in start_y..end_y {
            if y >= area.y && y < area.y + area.height {
                buf.set_string(x, y, "│", style);
            }
        }

        // Draw arrow at bottom
        if end_y > 0 && end_y - 1 >= area.y && end_y - 1 < area.y + area.height {
            buf.set_string(x, end_y - 1, "▼", style);
        }

        // Draw binding label
        if let Some(ref binding) = self.binding {
            let mid_y = (start_y + end_y) / 2;
            if mid_y >= area.y && mid_y < area.y + area.height {
                let label_x = x.saturating_sub(binding.len() as u16 / 2);
                if label_x >= area.x && label_x + binding.len() as u16 <= area.x + area.width {
                    buf.set_string(label_x + 2, mid_y, binding, style);
                }
            }
        }

        // Draw preview (grey, offset to the right)
        if let Some(ref preview) = self.preview {
            let mid_y = (start_y + end_y) / 2;
            if mid_y >= area.y && mid_y < area.y + area.height {
                let preview_x = x + 2;
                if let Some(binding) = &self.binding {
                    let preview_x = x + binding.len() as u16 + 4;
                    let truncated = if preview.len() > 25 {
                        format!("░░ {}... ░░", &preview[..22])
                    } else {
                        format!("░░ {} ░░", preview)
                    };
                    if preview_x + truncated.len() as u16 <= area.x + area.width {
                        buf.set_string(preview_x, mid_y, &truncated, preview_style);
                    }
                }
            }
        }
    }
}

/// Render merge point where multiple edges combine
pub fn render_merge(
    sources: &[(u16, u16)],
    target: (u16, u16),
    buf: &mut Buffer,
    area: Rect,
) {
    if sources.is_empty() {
        return;
    }

    let style = Style::default().fg(Color::DarkGray);
    let target_x = target.0;
    let target_y = target.1;

    // Draw horizontal lines from each source to the merge point
    let merge_y = target_y.saturating_sub(1);

    for (sx, sy) in sources {
        if *sy >= area.y && *sy < area.y + area.height {
            // Vertical line down from source
            for y in (*sy + 1)..merge_y {
                if y >= area.y && y < area.y + area.height {
                    buf.set_string(*sx, y, "│", style);
                }
            }

            // Horizontal line to merge point
            if merge_y >= area.y && merge_y < area.y + area.height {
                let (start_x, end_x) = if *sx < target_x {
                    (*sx, target_x)
                } else {
                    (target_x, *sx)
                };

                for x in start_x..=end_x {
                    if x >= area.x && x < area.x + area.width {
                        let ch = if x == *sx {
                            if *sx < target_x { "└" } else { "┘" }
                        } else if x == target_x {
                            "┬"
                        } else {
                            "─"
                        };
                        buf.set_string(x, merge_y, ch, style);
                    }
                }
            }
        }
    }

    // Arrow down to target
    if target_y >= area.y && target_y < area.y + area.height {
        buf.set_string(target_x, target_y - 1, "│", style);
    }
}
```

**Step 2: Tests and export, commit**

```bash
cargo test --features tui dag_edge
git add src/tui/widgets/dag_edge.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add DagEdge rendering with binding labels and previews

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: dag_ascii.rs - Main Widget

**Files:**
- Create: `src/tui/widgets/dag_ascii.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create main DagAscii widget**

```rust
//! DAG ASCII Visualizer
//!
//! Main widget that composes layout, node boxes, and edges
//! into a complete DAG visualization.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use super::{
    dag_edge::{DagEdge, render_merge},
    dag_layout::{DagLayout, LayoutConfig, NodePosition},
    dag_node_box::{NodeBox, NodeBoxData, NodeBoxMode},
};
use crate::tui::theme::VerbColor;

/// Complete DAG ASCII visualization
pub struct DagAscii<'a> {
    /// Nodes to render
    nodes: &'a [NodeBoxData],
    /// Dependencies per node (node_id -> [dep_ids])
    dependencies: HashMap<String, Vec<String>>,
    /// Binding labels per edge (from_id -> [(to_id, binding)])
    bindings: HashMap<String, Vec<(String, String)>>,
    /// Data previews per binding (binding -> preview)
    previews: HashMap<String, String>,
    /// View mode
    mode: NodeBoxMode,
    /// Animation frame
    frame: u8,
    /// Scroll offset
    scroll: (u16, u16),
}

impl<'a> DagAscii<'a> {
    pub fn new(nodes: &'a [NodeBoxData]) -> Self {
        Self {
            nodes,
            dependencies: HashMap::new(),
            bindings: HashMap::new(),
            previews: HashMap::new(),
            mode: NodeBoxMode::Minimal,
            frame: 0,
            scroll: (0, 0),
        }
    }

    pub fn with_dependencies(mut self, deps: HashMap<String, Vec<String>>) -> Self {
        self.dependencies = deps;
        self
    }

    pub fn with_bindings(mut self, bindings: HashMap<String, Vec<(String, String)>>) -> Self {
        self.bindings = bindings;
        self
    }

    pub fn with_previews(mut self, previews: HashMap<String, String>) -> Self {
        self.previews = previews;
        self
    }

    pub fn mode(mut self, mode: NodeBoxMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    pub fn scroll(mut self, x: u16, y: u16) -> Self {
        self.scroll = (x, y);
        self
    }
}

impl Widget for DagAscii<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 5 || self.nodes.is_empty() {
            buf.set_string(
                area.x + 2,
                area.y + 1,
                "(no tasks)",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        // Prepare node data for layout
        let node_deps: Vec<(String, Vec<String>)> = self
            .nodes
            .iter()
            .map(|n| {
                let deps = self.dependencies.get(&n.id).cloned().unwrap_or_default();
                (n.id.clone(), deps)
            })
            .collect();

        // Compute node widths
        let node_widths: HashMap<String, u16> = self
            .nodes
            .iter()
            .map(|n| {
                let widget = NodeBox::new(n).mode(self.mode);
                (n.id.clone(), widget.required_width())
            })
            .collect();

        // Compute layout
        let config = LayoutConfig {
            expanded: self.mode == NodeBoxMode::Expanded,
            ..Default::default()
        };
        let layout = DagLayout::compute(&node_deps, &config, &node_widths);

        // Render edges first (below nodes)
        for node in self.nodes {
            if let Some(pos) = layout.get(&node.id) {
                let deps = self.dependencies.get(&node.id).cloned().unwrap_or_default();

                if deps.len() == 1 {
                    // Single dependency - simple edge
                    if let Some(dep_pos) = layout.get(&deps[0]) {
                        let from = (
                            area.x + dep_pos.x + dep_pos.width / 2 - self.scroll.0,
                            area.y + dep_pos.y + dep_pos.height - self.scroll.1,
                        );
                        let to = (
                            area.x + pos.x + pos.width / 2 - self.scroll.0,
                            area.y + pos.y - self.scroll.1,
                        );

                        let mut edge = DagEdge::new(from, to);

                        // Add binding label if exists
                        if let Some(bindings) = self.bindings.get(&deps[0]) {
                            for (target, binding) in bindings {
                                if target == &node.id {
                                    edge = edge.with_binding(format!("{{{{{}}}}}", binding));
                                    if let Some(preview) = self.previews.get(binding) {
                                        edge = edge.with_preview(preview.clone());
                                    }
                                }
                            }
                        }

                        edge.render(buf, area);
                    }
                } else if deps.len() > 1 {
                    // Multiple dependencies - merge point
                    let sources: Vec<(u16, u16)> = deps
                        .iter()
                        .filter_map(|d| layout.get(d))
                        .map(|p| {
                            (
                                area.x + p.x + p.width / 2 - self.scroll.0,
                                area.y + p.y + p.height - self.scroll.1,
                            )
                        })
                        .collect();

                    let target = (
                        area.x + pos.x + pos.width / 2 - self.scroll.0,
                        area.y + pos.y - self.scroll.1,
                    );

                    render_merge(&sources, target, buf, area);
                }
            }
        }

        // Render nodes
        for node in self.nodes {
            if let Some(pos) = layout.get(&node.id) {
                let node_area = Rect {
                    x: area.x + pos.x.saturating_sub(self.scroll.0),
                    y: area.y + pos.y.saturating_sub(self.scroll.1),
                    width: pos.width.min(area.width),
                    height: pos.height.min(area.height),
                };

                // Only render if visible
                if node_area.x < area.x + area.width && node_area.y < area.y + area.height {
                    NodeBox::new(node)
                        .mode(self.mode)
                        .frame(self.frame)
                        .render(node_area, buf);
                }
            }
        }

        // Render stats footer
        let stats_y = area.y + area.height - 1;
        let stats = format!(
            "📊 {} tasks · {} layers",
            self.nodes.len(),
            layout.layer_count()
        );
        buf.set_string(
            area.x + 2,
            stats_y,
            &stats,
            Style::default().fg(Color::DarkGray),
        );
    }
}
```

**Step 2: Tests and commit**

```bash
cargo test --features tui dag_ascii
git add src/tui/widgets/dag_ascii.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add DagAscii main widget composing layout, nodes, edges

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Update browser.rs

**Files:**
- Modify: `src/tui/views/browser.rs`

**Step 1: Import new widgets**

```rust
use crate::tui::widgets::{
    DagAscii, NodeBoxData, NodeBoxMode,
    // ... existing imports
};
use crate::tui::theme::VerbColor;
```

**Step 2: Add expanded state and keybinding**

```rust
pub struct BrowserView {
    // ... existing fields
    /// DAG expanded mode
    pub dag_expanded: bool,
}

impl BrowserView {
    /// Toggle DAG expanded/minimal mode
    pub fn toggle_dag_mode(&mut self) {
        self.dag_expanded = !self.dag_expanded;
    }
}
```

**Step 3: Replace generate_dag_ascii with DagAscii widget**

Update `render_dag_preview` to use the new widget instead of generating ASCII strings.

**Step 4: Add 'E' keybinding in app.rs**

Handle 'E' key to call `browser_view.toggle_dag_mode()`.

**Step 5: Test and commit**

```bash
cargo test --features tui browser
cargo run --features tui -- tui examples/
# Press 'E' to toggle expanded mode
git add src/tui/views/browser.rs src/tui/app.rs
git commit -m "feat(tui): integrate DagAscii widget in browser view

- Replace ASCII string generation with DagAscii widget
- Add 'E' keybinding to toggle expanded/minimal mode
- Use verb colors for node borders

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Verification

After all tasks complete:

```bash
# Run all tests
cargo test --features tui

# Manual verification
cargo run --features tui -- tui examples/

# Check:
# - [ ] DAG preview shows colored borders per verb
# - [ ] 'E' toggles expanded/minimal mode
# - [ ] Binding labels visible on edges
# - [ ] Parallel tasks shown side by side
# - [ ] for_each tasks show mini-nodes
```

---

## Summary

| Task | Files | Description |
|------|-------|-------------|
| 1 | theme.rs | VerbColor enum with 5 colors |
| 2 | dag_layout.rs | Sugiyama layout algorithm |
| 3 | dag_node_box.rs | Node box rendering |
| 4 | dag_edge.rs | Edge rendering with labels |
| 5 | dag_ascii.rs | Main widget composition |
| 6 | browser.rs, app.rs | Integration + keybindings |
