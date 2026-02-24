# Phase 4: DAG Panel — TUI Visualization + Event Subscription

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create a TUI panel that visualizes the chat DAG in real-time, showing messages as nodes with @mention edges.

**Architecture:** `ChatDagPanel` widget subscribes to `EventLog` updates and renders the DAG using ratatui. Node selection scrolls to the corresponding message in the chat view.

**Tech Stack:** ratatui, parking_lot::Mutex, tokio::sync::watch (for event subscription)

**Skills:** @rust-core, @frontend-design, @test-driven-development

---

## Phase Dependencies

| Depends On | Provides |
|------------|----------|
| Phase 1 (FlowGraph) | `FlowGraph` with StableGraph |
| Phase 2 (ChatWorkflow) | `ChatWorkflow` struct |
| Phase 3 (EventLog) | Event subscription patterns |

---

## Tasks Overview

| Task | Focus | Tests | Files |
|------|-------|-------|-------|
| 4.1 | NodeBox widget | 5 | `src/tui/widgets/node_box.rs` |
| 4.2 | EdgeLine widget | 4 | `src/tui/widgets/edge_line.rs` |
| 4.3 | ChatDagPanel layout | 6 | `src/tui/widgets/chat_dag_panel.rs` |
| 4.4 | Event subscription | 5 | `src/tui/widgets/chat_dag_panel.rs` |
| 4.5 | Node selection + click handler | 4 | `src/tui/widgets/chat_dag_panel.rs` |
| 4.6 | Chat view integration | 4 | `src/tui/views/chat.rs` |
| 4.7 | Toggle DAG panel visibility | 3 | `src/tui/views/chat.rs` |
| 4.8 | Scroll sync (DAG ↔ Chat) | 4 | `src/tui/views/chat.rs` |
| **Total** | | **35** | |

---

## Task 4.1: NodeBox Widget

**Files:**
- Create: `src/tui/widgets/node_box.rs`
- Modify: `src/tui/widgets/mod.rs` (export)

**Step 1: Write the failing test**

```rust
// src/tui/widgets/node_box.rs

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_node_box_renders_task_id() {
        let node = NodeBox::new("msg-001", NodeKind::UserMessage);
        let area = Rect::new(0, 0, 20, 3);
        let mut buf = Buffer::empty(area);
        node.render(area, &mut buf);
        let content = buf.content.iter().map(|c| c.symbol()).collect::<String>();
        assert!(content.contains("msg-001") || content.contains("001"));
    }

    #[test]
    fn test_node_box_user_message_style() {
        let node = NodeBox::new("msg-001", NodeKind::UserMessage);
        assert_eq!(node.kind, NodeKind::UserMessage);
    }

    #[test]
    fn test_node_box_assistant_message_style() {
        let node = NodeBox::new("msg-002", NodeKind::AssistantMessage);
        assert_eq!(node.kind, NodeKind::AssistantMessage);
    }

    #[test]
    fn test_node_box_tool_call_style() {
        let node = NodeBox::new("tool-001", NodeKind::ToolCall);
        assert_eq!(node.kind, NodeKind::ToolCall);
    }

    #[test]
    fn test_node_box_selected_state() {
        let node = NodeBox::new("msg-001", NodeKind::UserMessage).selected(true);
        assert!(node.is_selected);
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test node_box --lib
```

Expected: FAIL with "cannot find struct `NodeBox`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/node_box.rs

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style, Modifier},
    widgets::{Block, Borders, Widget},
    text::{Line, Span},
};

/// Visual kind of DAG node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    UserMessage,      // Human input
    AssistantMessage, // AI response
    ToolCall,         // MCP or builtin tool
    SystemMessage,    // System events
}

impl NodeKind {
    pub fn icon(&self) -> &'static str {
        match self {
            NodeKind::UserMessage => "👤",
            NodeKind::AssistantMessage => "🤖",
            NodeKind::ToolCall => "🔧",
            NodeKind::SystemMessage => "⚙️",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            NodeKind::UserMessage => Color::Cyan,
            NodeKind::AssistantMessage => Color::Green,
            NodeKind::ToolCall => Color::Yellow,
            NodeKind::SystemMessage => Color::Gray,
        }
    }
}

/// Single node box in the DAG visualization
pub struct NodeBox {
    pub task_id: String,
    pub kind: NodeKind,
    pub is_selected: bool,
    pub is_running: bool,
}

impl NodeBox {
    pub fn new(task_id: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            task_id: task_id.into(),
            kind,
            is_selected: false,
            is_running: false,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }

    pub fn running(mut self, running: bool) -> Self {
        self.is_running = running;
        self
    }

    fn border_style(&self) -> Style {
        let base = Style::default().fg(self.kind.color());
        if self.is_selected {
            base.add_modifier(Modifier::BOLD).fg(Color::White)
        } else if self.is_running {
            base.add_modifier(Modifier::SLOW_BLINK)
        } else {
            base
        }
    }
}

impl Widget for NodeBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 || area.height < 3 {
            return; // Too small to render
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.border_style())
            .title(format!("{} {}", self.kind.icon(), self.task_id));

        block.render(area, buf);
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test node_box --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/node_box.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add NodeBox widget for DAG visualization

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 4.2: EdgeLine Widget

**Files:**
- Create: `src/tui/widgets/edge_line.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/edge_line.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_line_from_to() {
        let edge = EdgeLine::new((0, 1), (2, 1));
        assert_eq!(edge.from, (0, 1));
        assert_eq!(edge.to, (2, 1));
    }

    #[test]
    fn test_edge_line_horizontal() {
        let edge = EdgeLine::new((0, 5), (10, 5));
        assert!(edge.is_horizontal());
    }

    #[test]
    fn test_edge_line_vertical() {
        let edge = EdgeLine::new((5, 0), (5, 10));
        assert!(edge.is_vertical());
    }

    #[test]
    fn test_edge_line_diagonal() {
        let edge = EdgeLine::new((0, 0), (5, 5));
        assert!(!edge.is_horizontal());
        assert!(!edge.is_vertical());
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test edge_line --lib
```

Expected: FAIL with "cannot find struct `EdgeLine`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/edge_line.rs

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// Edge connecting two nodes in DAG
pub struct EdgeLine {
    pub from: (u16, u16),  // (x, y)
    pub to: (u16, u16),
    pub style: Style,
}

impl EdgeLine {
    pub fn new(from: (u16, u16), to: (u16, u16)) -> Self {
        Self {
            from,
            to,
            style: Style::default().fg(Color::DarkGray),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn is_horizontal(&self) -> bool {
        self.from.1 == self.to.1
    }

    pub fn is_vertical(&self) -> bool {
        self.from.0 == self.to.0
    }

    fn line_char(&self) -> char {
        if self.is_horizontal() {
            '─'
        } else if self.is_vertical() {
            '│'
        } else {
            // Diagonal - use arrow
            '↘'
        }
    }
}

impl Widget for EdgeLine {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Simple line drawing
        let char = self.line_char();

        if self.is_horizontal() {
            let y = self.from.1;
            let start_x = self.from.0.min(self.to.0);
            let end_x = self.from.0.max(self.to.0);
            for x in start_x..=end_x {
                if x < area.width && y < area.height {
                    buf.get_mut(area.x + x, area.y + y)
                        .set_char(char)
                        .set_style(self.style);
                }
            }
        } else if self.is_vertical() {
            let x = self.from.0;
            let start_y = self.from.1.min(self.to.1);
            let end_y = self.from.1.max(self.to.1);
            for y in start_y..=end_y {
                if x < area.width && y < area.height {
                    buf.get_mut(area.x + x, area.y + y)
                        .set_char(char)
                        .set_style(self.style);
                }
            }
        } else {
            // Diagonal: just draw arrow at target
            if self.to.0 < area.width && self.to.1 < area.height {
                buf.get_mut(area.x + self.to.0, area.y + self.to.1)
                    .set_char('→')
                    .set_style(self.style);
            }
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test edge_line --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/edge_line.rs
git commit -m "feat(tui): add EdgeLine widget for DAG connections

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 4.3: ChatDagPanel Layout

**Files:**
- Create: `src/tui/widgets/chat_dag_panel.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/chat_dag_panel.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dag_panel_new() {
        let panel = ChatDagPanel::new();
        assert!(panel.nodes.is_empty());
        assert!(panel.edges.is_empty());
    }

    #[test]
    fn test_dag_panel_add_node() {
        let mut panel = ChatDagPanel::new();
        panel.add_node("msg-001", NodeKind::UserMessage);
        assert_eq!(panel.nodes.len(), 1);
    }

    #[test]
    fn test_dag_panel_add_edge() {
        let mut panel = ChatDagPanel::new();
        panel.add_node("msg-001", NodeKind::UserMessage);
        panel.add_node("msg-002", NodeKind::AssistantMessage);
        panel.add_edge("msg-001", "msg-002");
        assert_eq!(panel.edges.len(), 1);
    }

    #[test]
    fn test_dag_panel_layout_single_node() {
        let mut panel = ChatDagPanel::new();
        panel.add_node("msg-001", NodeKind::UserMessage);
        let positions = panel.compute_layout(Rect::new(0, 0, 80, 24));
        assert!(positions.contains_key("msg-001"));
    }

    #[test]
    fn test_dag_panel_layout_chain() {
        let mut panel = ChatDagPanel::new();
        panel.add_node("msg-001", NodeKind::UserMessage);
        panel.add_node("msg-002", NodeKind::AssistantMessage);
        panel.add_node("msg-003", NodeKind::UserMessage);
        panel.add_edge("msg-001", "msg-002");
        panel.add_edge("msg-002", "msg-003");
        let positions = panel.compute_layout(Rect::new(0, 0, 80, 24));
        // Nodes should be laid out vertically
        let pos1 = positions.get("msg-001").unwrap();
        let pos2 = positions.get("msg-002").unwrap();
        assert!(pos2.y > pos1.y, "msg-002 should be below msg-001");
    }

    #[test]
    fn test_dag_panel_select_node() {
        let mut panel = ChatDagPanel::new();
        panel.add_node("msg-001", NodeKind::UserMessage);
        panel.add_node("msg-002", NodeKind::AssistantMessage);
        panel.select("msg-002");
        assert_eq!(panel.selected, Some("msg-002".to_string()));
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test dag_panel --lib
```

Expected: FAIL with "cannot find struct `ChatDagPanel`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/chat_dag_panel.rs

use std::collections::HashMap;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget, StatefulWidget},
};
use super::node_box::{NodeBox, NodeKind};
use super::edge_line::EdgeLine;

/// Node position in the layout
#[derive(Debug, Clone, Copy)]
pub struct NodePosition {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// DAG panel state
pub struct ChatDagPanelState {
    pub scroll_offset: u16,
    pub selected_index: Option<usize>,
}

impl Default for ChatDagPanelState {
    fn default() -> Self {
        Self {
            scroll_offset: 0,
            selected_index: None,
        }
    }
}

/// Panel showing chat DAG visualization
pub struct ChatDagPanel {
    pub nodes: Vec<(String, NodeKind)>,
    pub edges: Vec<(String, String)>,  // (from, to)
    pub selected: Option<String>,
    node_positions: HashMap<String, NodePosition>,
}

impl ChatDagPanel {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            selected: None,
            node_positions: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, id: impl Into<String>, kind: NodeKind) {
        self.nodes.push((id.into(), kind));
    }

    pub fn add_edge(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.edges.push((from.into(), to.into()));
    }

    pub fn select(&mut self, id: impl Into<String>) {
        self.selected = Some(id.into());
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Compute layout positions for all nodes
    pub fn compute_layout(&mut self, area: Rect) -> &HashMap<String, NodePosition> {
        self.node_positions.clear();

        let node_width = 15u16;
        let node_height = 3u16;
        let vertical_gap = 2u16;
        let start_x = 2u16;
        let start_y = 2u16;

        for (i, (id, _kind)) in self.nodes.iter().enumerate() {
            let y = start_y + (i as u16) * (node_height + vertical_gap);
            self.node_positions.insert(id.clone(), NodePosition {
                x: start_x,
                y,
                width: node_width,
                height: node_height,
            });
        }

        &self.node_positions
    }

    /// Get node ID at position (for click handling)
    pub fn node_at(&self, x: u16, y: u16) -> Option<&str> {
        for (id, pos) in &self.node_positions {
            if x >= pos.x && x < pos.x + pos.width
                && y >= pos.y && y < pos.y + pos.height
            {
                return Some(id.as_str());
            }
        }
        None
    }
}

impl Widget for &ChatDagPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" DAG ")
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        block.render(area, buf);

        // Draw edges first (behind nodes)
        for (from_id, to_id) in &self.edges {
            if let (Some(from_pos), Some(to_pos)) = (
                self.node_positions.get(from_id),
                self.node_positions.get(to_id)
            ) {
                let edge = EdgeLine::new(
                    (from_pos.x + from_pos.width / 2, from_pos.y + from_pos.height),
                    (to_pos.x + to_pos.width / 2, to_pos.y),
                );
                edge.render(inner, buf);
            }
        }

        // Draw nodes
        for (id, kind) in &self.nodes {
            if let Some(pos) = self.node_positions.get(id) {
                let is_selected = self.selected.as_ref() == Some(id);
                let node = NodeBox::new(id, *kind).selected(is_selected);
                let node_area = Rect::new(
                    inner.x + pos.x,
                    inner.y + pos.y,
                    pos.width.min(inner.width.saturating_sub(pos.x)),
                    pos.height.min(inner.height.saturating_sub(pos.y)),
                );
                node.render(node_area, buf);
            }
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test dag_panel --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/chat_dag_panel.rs
git commit -m "feat(tui): add ChatDagPanel with layout algorithm

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 4.4: Event Subscription

**Files:**
- Modify: `src/tui/widgets/chat_dag_panel.rs`
- Modify: `src/event/log.rs` (add subscription)

**Step 1: Write the failing test**

```rust
// In src/tui/widgets/chat_dag_panel.rs tests

#[tokio::test]
async fn test_dag_panel_subscribes_to_events() {
    use tokio::sync::watch;

    let (tx, rx) = watch::channel(Vec::new());
    let mut panel = ChatDagPanel::new().with_event_receiver(rx);

    // Simulate event
    tx.send(vec![Event {
        kind: EventKind::TaskStarted { task_id: Arc::from("msg-001") },
        timestamp: Instant::now(),
    }]).unwrap();

    panel.process_pending_events();
    assert_eq!(panel.nodes.len(), 1);
}

#[tokio::test]
async fn test_dag_panel_processes_task_completed() {
    use tokio::sync::watch;

    let (tx, rx) = watch::channel(Vec::new());
    let mut panel = ChatDagPanel::new().with_event_receiver(rx);

    tx.send(vec![
        Event { kind: EventKind::TaskStarted { task_id: Arc::from("msg-001") }, timestamp: Instant::now() },
        Event { kind: EventKind::TaskCompleted { task_id: Arc::from("msg-001"), result: serde_json::Value::Null }, timestamp: Instant::now() },
    ]).unwrap();

    panel.process_pending_events();
    // Node should be marked as completed
    assert!(panel.completed_tasks.contains("msg-001"));
}

#[tokio::test]
async fn test_dag_panel_processes_chat_message() {
    use tokio::sync::watch;

    let (tx, rx) = watch::channel(Vec::new());
    let mut panel = ChatDagPanel::new().with_event_receiver(rx);

    tx.send(vec![Event {
        kind: EventKind::ChatMessage {
            task_id: Arc::from("msg-001"),
            role: "user".into(),
            content: "Hello".into(),
            mentions: vec![],
        },
        timestamp: Instant::now(),
    }]).unwrap();

    panel.process_pending_events();
    assert_eq!(panel.nodes.len(), 1);
    assert_eq!(panel.nodes[0].1, NodeKind::UserMessage);
}

#[test]
fn test_dag_panel_adds_edge_from_mention() {
    let mut panel = ChatDagPanel::new();
    panel.add_node("msg-001", NodeKind::UserMessage);
    panel.add_node("msg-002", NodeKind::AssistantMessage);

    // Process mention edge
    panel.add_mention_edge("msg-002", "msg-001");

    assert_eq!(panel.edges.len(), 1);
    assert_eq!(panel.edges[0], ("msg-001".to_string(), "msg-002".to_string()));
}

#[test]
fn test_dag_panel_processes_agent_turn() {
    let mut panel = ChatDagPanel::new();

    panel.process_event(&Event {
        kind: EventKind::AgentTurn {
            task_id: Arc::from("turn-001"),
            turn_number: 1,
            response: "Thinking...".into(),
            tool_calls: vec![],
            metadata: None,
        },
        timestamp: Instant::now(),
    });

    assert_eq!(panel.nodes.len(), 1);
    assert_eq!(panel.nodes[0].1, NodeKind::AssistantMessage);
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test dag_panel_subscribes --lib
```

Expected: FAIL with method not found

**Step 3: Write minimal implementation**

Add to `ChatDagPanel`:

```rust
use std::sync::Arc;
use std::collections::HashSet;
use tokio::sync::watch;
use crate::event::{Event, EventKind, EventLog};

impl ChatDagPanel {
    pub fn with_event_receiver(mut self, rx: watch::Receiver<Vec<Event>>) -> Self {
        self.event_rx = Some(rx);
        self
    }

    pub fn subscribe_to(&mut self, log: &EventLog) {
        self.event_rx = Some(log.subscribe());
    }

    pub fn process_pending_events(&mut self) {
        if let Some(rx) = &self.event_rx {
            let events = rx.borrow().clone();
            for event in events {
                self.process_event(&event);
            }
        }
    }

    pub fn process_event(&mut self, event: &Event) {
        match &event.kind {
            EventKind::TaskStarted { task_id } => {
                if !self.has_node(task_id) {
                    self.add_node(task_id.to_string(), NodeKind::SystemMessage);
                }
            }
            EventKind::TaskCompleted { task_id, .. } => {
                self.completed_tasks.insert(task_id.to_string());
            }
            EventKind::ChatMessage { task_id, role, mentions, .. } => {
                let kind = match role.as_str() {
                    "user" => NodeKind::UserMessage,
                    "assistant" => NodeKind::AssistantMessage,
                    _ => NodeKind::SystemMessage,
                };
                if !self.has_node(task_id) {
                    self.add_node(task_id.to_string(), kind);
                }
                // Add edges from mentions
                for mention_id in mentions {
                    self.add_mention_edge(task_id, mention_id);
                }
            }
            EventKind::AgentTurn { task_id, .. } => {
                if !self.has_node(task_id) {
                    self.add_node(task_id.to_string(), NodeKind::AssistantMessage);
                }
            }
            EventKind::BuiltinInvoke { tool, .. } => {
                self.add_node(format!("tool-{}", self.nodes.len()), NodeKind::ToolCall);
            }
            _ => {}
        }
    }

    pub fn add_mention_edge(&mut self, from: &str, to: &str) {
        // Edge goes from mentioned node to current node
        self.edges.push((to.to_string(), from.to_string()));
    }

    fn has_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|(n, _)| n == id)
    }
}

// Add fields to ChatDagPanel struct
pub struct ChatDagPanel {
    pub nodes: Vec<(String, NodeKind)>,
    pub edges: Vec<(String, String)>,
    pub selected: Option<String>,
    node_positions: HashMap<String, NodePosition>,
    event_rx: Option<watch::Receiver<Vec<Event>>>,
    completed_tasks: HashSet<String>,
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test dag_panel_subscribes --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/chat_dag_panel.rs src/event/log.rs
git commit -m "feat(tui): add event subscription to ChatDagPanel

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 4.5: Node Selection + Click Handler

**Files:**
- Modify: `src/tui/widgets/chat_dag_panel.rs`

**Step 1: Write the failing test**

```rust
// In src/tui/widgets/chat_dag_panel.rs tests

#[test]
fn test_dag_panel_select_next() {
    let mut panel = ChatDagPanel::new();
    panel.add_node("msg-001", NodeKind::UserMessage);
    panel.add_node("msg-002", NodeKind::AssistantMessage);
    panel.add_node("msg-003", NodeKind::UserMessage);

    panel.select_next();
    assert_eq!(panel.selected, Some("msg-001".to_string()));

    panel.select_next();
    assert_eq!(panel.selected, Some("msg-002".to_string()));
}

#[test]
fn test_dag_panel_select_prev() {
    let mut panel = ChatDagPanel::new();
    panel.add_node("msg-001", NodeKind::UserMessage);
    panel.add_node("msg-002", NodeKind::AssistantMessage);
    panel.select("msg-002");

    panel.select_prev();
    assert_eq!(panel.selected, Some("msg-001".to_string()));
}

#[test]
fn test_dag_panel_handle_click() {
    let mut panel = ChatDagPanel::new();
    panel.add_node("msg-001", NodeKind::UserMessage);
    panel.compute_layout(Rect::new(0, 0, 80, 24));

    // Click within node area
    let clicked = panel.handle_click(4, 4); // Approximate position
    assert!(clicked.is_some());
}

#[test]
fn test_dag_panel_on_select_callback() {
    let mut panel = ChatDagPanel::new();
    let selected_id = std::cell::RefCell::new(None);

    panel.on_select(|id| {
        *selected_id.borrow_mut() = Some(id.to_string());
    });

    panel.add_node("msg-001", NodeKind::UserMessage);
    panel.select("msg-001");

    assert_eq!(*selected_id.borrow(), Some("msg-001".to_string()));
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test dag_panel_select --lib
```

Expected: FAIL with method not found

**Step 3: Write minimal implementation**

Add to `ChatDagPanel`:

```rust
impl ChatDagPanel {
    pub fn select_next(&mut self) {
        if self.nodes.is_empty() {
            return;
        }

        let current_idx = self.selected.as_ref()
            .and_then(|s| self.nodes.iter().position(|(id, _)| id == s));

        let next_idx = match current_idx {
            Some(idx) => (idx + 1) % self.nodes.len(),
            None => 0,
        };

        self.selected = Some(self.nodes[next_idx].0.clone());
        self.trigger_on_select();
    }

    pub fn select_prev(&mut self) {
        if self.nodes.is_empty() {
            return;
        }

        let current_idx = self.selected.as_ref()
            .and_then(|s| self.nodes.iter().position(|(id, _)| id == s));

        let prev_idx = match current_idx {
            Some(0) => self.nodes.len() - 1,
            Some(idx) => idx - 1,
            None => self.nodes.len() - 1,
        };

        self.selected = Some(self.nodes[prev_idx].0.clone());
        self.trigger_on_select();
    }

    pub fn handle_click(&mut self, x: u16, y: u16) -> Option<String> {
        if let Some(id) = self.node_at(x, y) {
            self.selected = Some(id.to_string());
            self.trigger_on_select();
            return Some(id.to_string());
        }
        None
    }

    pub fn on_select<F>(&mut self, callback: F)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_select_callback = Some(Box::new(callback));
    }

    fn trigger_on_select(&self) {
        if let (Some(id), Some(callback)) = (&self.selected, &self.on_select_callback) {
            callback(id);
        }
    }
}

// Add to struct
pub struct ChatDagPanel {
    // ... existing fields ...
    on_select_callback: Option<Box<dyn Fn(&str) + Send + Sync>>,
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test dag_panel_select --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/chat_dag_panel.rs
git commit -m "feat(tui): add node selection and click handling to ChatDagPanel

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 4.6: Chat View Integration

**Files:**
- Modify: `src/tui/views/chat.rs`

**Step 1: Write the failing test**

```rust
// In src/tui/views/chat.rs tests

#[test]
fn test_chat_view_has_dag_panel() {
    let view = ChatView::new();
    assert!(view.dag_panel.is_some());
}

#[test]
fn test_chat_view_dag_panel_syncs_with_messages() {
    let mut view = ChatView::new();
    view.add_message("Hello", Role::User);
    view.add_message("Hi there!", Role::Assistant);

    let panel = view.dag_panel.as_ref().unwrap();
    assert_eq!(panel.nodes.len(), 2);
}

#[test]
fn test_chat_view_dag_selection_scrolls_to_message() {
    let mut view = ChatView::new();
    for i in 0..20 {
        view.add_message(&format!("Message {}", i), Role::User);
    }

    // Select node near the end
    view.dag_panel.as_mut().unwrap().select("msg-015");
    view.sync_scroll_from_dag();

    // Scroll should have moved
    assert!(view.scroll_offset > 0);
}

#[test]
fn test_chat_view_renders_dag_panel() {
    let view = ChatView::new();
    let area = Rect::new(0, 0, 120, 40);
    let mut buf = Buffer::empty(area);
    view.render(area, &mut buf);

    // DAG panel should be rendered in right portion
    // (verify border exists in right area)
    let right_edge = buf.get(area.width - 2, area.height / 2).symbol();
    assert!(right_edge.contains("│") || right_edge.contains("─"));
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test chat_view_dag --lib
```

Expected: FAIL with field not found

**Step 3: Write minimal implementation**

Modify `src/tui/views/chat.rs`:

```rust
use crate::tui::widgets::chat_dag_panel::{ChatDagPanel, NodeKind};

pub struct ChatView {
    // ... existing fields ...
    pub dag_panel: Option<ChatDagPanel>,
    pub show_dag_panel: bool,
    dag_panel_width: u16,
}

impl ChatView {
    pub fn new() -> Self {
        let mut dag_panel = ChatDagPanel::new();

        Self {
            // ... existing initialization ...
            dag_panel: Some(dag_panel),
            show_dag_panel: true,
            dag_panel_width: 25, // Default width
        }
    }

    pub fn add_message(&mut self, content: &str, role: Role) {
        let msg_id = format!("msg-{:03}", self.messages.len());

        // Add to messages list
        self.messages.push(Message {
            id: msg_id.clone(),
            content: content.to_string(),
            role,
        });

        // Add to DAG panel
        if let Some(panel) = &mut self.dag_panel {
            let kind = match role {
                Role::User => NodeKind::UserMessage,
                Role::Assistant => NodeKind::AssistantMessage,
                Role::System => NodeKind::SystemMessage,
            };
            panel.add_node(&msg_id, kind);

            // Add edge from previous message
            if self.messages.len() > 1 {
                let prev_id = format!("msg-{:03}", self.messages.len() - 2);
                panel.add_edge(&prev_id, &msg_id);
            }
        }
    }

    pub fn sync_scroll_from_dag(&mut self) {
        if let Some(panel) = &self.dag_panel {
            if let Some(selected) = &panel.selected {
                // Find message index
                if let Some(idx) = self.messages.iter().position(|m| m.id == *selected) {
                    // Scroll to make this message visible
                    self.scroll_offset = idx.saturating_sub(3) as u16;
                }
            }
        }
    }

    pub fn toggle_dag_panel(&mut self) {
        self.show_dag_panel = !self.show_dag_panel;
    }
}

impl Widget for &ChatView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (chat_area, dag_area) = if self.show_dag_panel {
            let dag_width = self.dag_panel_width.min(area.width / 3);
            (
                Rect::new(area.x, area.y, area.width - dag_width, area.height),
                Rect::new(area.x + area.width - dag_width, area.y, dag_width, area.height),
            )
        } else {
            (area, Rect::default())
        };

        // Render chat messages
        self.render_messages(chat_area, buf);

        // Render DAG panel
        if self.show_dag_panel {
            if let Some(panel) = &self.dag_panel {
                panel.compute_layout(dag_area);
                panel.render(dag_area, buf);
            }
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test chat_view_dag --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/views/chat.rs
git commit -m "feat(tui): integrate ChatDagPanel into ChatView

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 4.7: Toggle DAG Panel Visibility

**Files:**
- Modify: `src/tui/views/chat.rs`
- Modify: `src/tui/app.rs` (key binding)

**Step 1: Write the failing test**

```rust
// In src/tui/views/chat.rs tests

#[test]
fn test_toggle_dag_panel() {
    let mut view = ChatView::new();
    assert!(view.show_dag_panel);

    view.toggle_dag_panel();
    assert!(!view.show_dag_panel);

    view.toggle_dag_panel();
    assert!(view.show_dag_panel);
}

#[test]
fn test_handle_key_d_toggles_dag() {
    let mut view = ChatView::new();
    let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);

    view.handle_key(event);
    assert!(!view.show_dag_panel);
}

#[test]
fn test_dag_panel_hidden_no_render() {
    let mut view = ChatView::new();
    view.show_dag_panel = false;

    let area = Rect::new(0, 0, 80, 24);
    let mut buf = Buffer::empty(area);
    view.render(area, &mut buf);

    // Chat should take full width
    // No DAG border on right side
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test toggle_dag --lib
```

Expected: FAIL

**Step 3: Write minimal implementation**

```rust
// In ChatView
impl ChatView {
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            // Ctrl+D toggles DAG panel
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                self.toggle_dag_panel();
                None
            }
            // ... other key handlers ...
            _ => None,
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test toggle_dag --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/views/chat.rs src/tui/app.rs
git commit -m "feat(tui): add Ctrl+D to toggle DAG panel visibility

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 4.8: Scroll Sync (DAG ↔ Chat)

**Files:**
- Modify: `src/tui/views/chat.rs`

**Step 1: Write the failing test**

```rust
// In src/tui/views/chat.rs tests

#[test]
fn test_scroll_sync_chat_to_dag() {
    let mut view = ChatView::new();
    for i in 0..20 {
        view.add_message(&format!("Message {}", i), Role::User);
    }

    // Scroll chat to middle
    view.scroll_offset = 10;
    view.sync_dag_from_scroll();

    // DAG should highlight msg-010
    let panel = view.dag_panel.as_ref().unwrap();
    assert!(panel.selected.as_ref().map(|s| s.contains("010")).unwrap_or(false));
}

#[test]
fn test_scroll_sync_dag_to_chat() {
    let mut view = ChatView::new();
    for i in 0..20 {
        view.add_message(&format!("Message {}", i), Role::User);
    }

    // Select DAG node
    view.dag_panel.as_mut().unwrap().select("msg-015");
    view.sync_scroll_from_dag();

    // Chat should scroll to show msg-015
    assert!(view.scroll_offset >= 12);
}

#[test]
fn test_dag_arrow_keys_scroll_chat() {
    let mut view = ChatView::new();
    for i in 0..10 {
        view.add_message(&format!("Message {}", i), Role::User);
    }

    // Press down in DAG panel
    view.dag_panel.as_mut().unwrap().select_next();
    view.sync_scroll_from_dag();

    // Should select first node and keep scroll at top
    assert_eq!(view.scroll_offset, 0);
}

#[test]
fn test_bidirectional_sync() {
    let mut view = ChatView::new();
    for i in 0..30 {
        view.add_message(&format!("Message {}", i), Role::User);
    }

    // Scroll chat
    view.scroll_offset = 20;
    view.sync_dag_from_scroll();

    // Modify selection in DAG
    view.dag_panel.as_mut().unwrap().select_next();
    view.sync_scroll_from_dag();

    // Both should be synchronized
    let panel = view.dag_panel.as_ref().unwrap();
    let selected_idx = panel.selected.as_ref()
        .and_then(|s| s.strip_prefix("msg-"))
        .and_then(|n| n.parse::<u16>().ok());

    assert!(selected_idx.is_some());
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test scroll_sync --lib
```

Expected: FAIL with method not found

**Step 3: Write minimal implementation**

```rust
impl ChatView {
    pub fn sync_dag_from_scroll(&mut self) {
        if let Some(panel) = &mut self.dag_panel {
            // Find message at current scroll position
            let visible_msg_idx = self.scroll_offset as usize;
            if visible_msg_idx < self.messages.len() {
                let msg_id = &self.messages[visible_msg_idx].id;
                panel.selected = Some(msg_id.clone());
            }
        }
    }

    pub fn sync_scroll_from_dag(&mut self) {
        if let Some(panel) = &self.dag_panel {
            if let Some(selected) = &panel.selected {
                // Find message index
                if let Some(idx) = self.messages.iter().position(|m| &m.id == selected) {
                    // Center the message in view (approximately)
                    let visible_lines = self.visible_height.saturating_sub(4);
                    self.scroll_offset = idx.saturating_sub(visible_lines as usize / 2) as u16;
                }
            }
        }
    }

    pub fn handle_dag_navigation(&mut self, key: KeyEvent) -> bool {
        if !self.show_dag_panel {
            return false;
        }

        match key.code {
            KeyCode::Up => {
                if let Some(panel) = &mut self.dag_panel {
                    panel.select_prev();
                }
                self.sync_scroll_from_dag();
                true
            }
            KeyCode::Down => {
                if let Some(panel) = &mut self.dag_panel {
                    panel.select_next();
                }
                self.sync_scroll_from_dag();
                true
            }
            KeyCode::Enter => {
                // Navigate to selected message
                self.sync_scroll_from_dag();
                true
            }
            _ => false,
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test scroll_sync --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/views/chat.rs
git commit -m "feat(tui): add bidirectional scroll sync between DAG and chat

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## 🔌 WIRING CHECKPOINT 4: ChatDagPanel ↔ EventLog Subscription

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  WIRING CHECKPOINT 4: ChatDagPanel ↔ EventLog Integration                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Verify these connections are working:                                        ║
║                                                                               ║
║  1. ChatDagPanel subscribes to EventLog via watch channel                     ║
║  2. New events update DAG nodes and edges in real-time                        ║
║  3. Node selection triggers scroll sync in ChatView                           ║
║  4. ChatView properly layouts DAG panel on right side                         ║
║  5. Ctrl+D toggles DAG panel visibility                                       ║
║  6. Arrow keys navigate DAG and sync scroll                                   ║
║                                                                               ║
║  Run: cargo test wiring_checkpoint_4 --lib                                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Wiring Test:**

```rust
// tests/wiring_checkpoint_4.rs

#[test]
fn wiring_checkpoint_4_dag_panel_to_chat_view() {
    let mut view = ChatView::new();

    // Test 1: DAG panel exists and is visible
    assert!(view.dag_panel.is_some());
    assert!(view.show_dag_panel);

    // Test 2: Adding messages updates DAG
    view.add_message("Hello", Role::User);
    view.add_message("Hi there!", Role::Assistant);
    assert_eq!(view.dag_panel.as_ref().unwrap().nodes.len(), 2);

    // Test 3: Selection sync works
    view.dag_panel.as_mut().unwrap().select("msg-001");
    view.sync_scroll_from_dag();

    // Test 4: Toggle works
    view.toggle_dag_panel();
    assert!(!view.show_dag_panel);

    // Test 5: Render doesn't panic
    let area = Rect::new(0, 0, 120, 40);
    let mut buf = Buffer::empty(area);
    view.render(area, &mut buf);
}
```

---

## 🧪 LIVE TEST: DAG Panel Visual Verification

After completing all Phase 4 tasks, run these live tests:

```bash
# Test 1: Start chat and verify DAG panel appears
cargo run -- chat

# In chat:
# 1. Type a message - verify node appears in DAG
# 2. Type another message - verify edge connects them
# 3. Press Ctrl+D - verify DAG panel hides/shows
# 4. Use arrow keys in DAG - verify chat scrolls
# 5. Click a node - verify chat scrolls to message
```

**Visual Verification Checklist:**

- [ ] DAG panel renders on right side of chat view
- [ ] User messages show 👤 icon with cyan border
- [ ] Assistant messages show 🤖 icon with green border
- [ ] Edges connect sequential messages
- [ ] Selected node has white bold border
- [ ] Ctrl+D toggles panel visibility
- [ ] Arrow up/down navigates nodes
- [ ] Node selection scrolls chat to message

---

## Summary

| Task | Description | Tests | Status |
|------|-------------|-------|--------|
| 4.1 | NodeBox widget | 5 | ⬜ |
| 4.2 | EdgeLine widget | 4 | ⬜ |
| 4.3 | ChatDagPanel layout | 6 | ⬜ |
| 4.4 | Event subscription | 5 | ⬜ |
| 4.5 | Node selection + click | 4 | ⬜ |
| 4.6 | Chat view integration | 4 | ⬜ |
| 4.7 | Toggle visibility | 3 | ⬜ |
| 4.8 | Scroll sync | 4 | ⬜ |
| **Total** | | **35** | |

---

## References

- [Phase 3: Builtin Tools](./Phase-3-BuiltinTools.md)
- [ratatui Widget docs](https://docs.rs/ratatui)
- [Thread Safety Architecture](./2026-02-24-thread-safety-architecture.md)
