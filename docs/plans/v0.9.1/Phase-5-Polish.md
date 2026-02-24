# Phase 5: Polish — Animations, Persistence, Export

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add polish features: node animations, DAG persistence across sessions, and export capabilities (JSON/Mermaid).

**Architecture:** Session state persists DAG to `.nika/sessions/`. Export generates Mermaid diagrams or JSON snapshots. Animations use tokio ticks for smooth transitions.

**Tech Stack:** serde_json, tokio (timers), ratatui (animations)

**Skills:** @rust-core, @test-driven-development, @verification-before-completion

---

## Phase Dependencies

| Depends On | Provides |
|------------|----------|
| Phase 4 (ChatDagPanel) | DAG visualization |
| Phase 1 (FlowGraph) | Graph serialization |
| v0.8.0 (Session System) | Session persistence infrastructure |

---

## Tasks Overview

| Task | Focus | Tests | Files |
|------|-------|-------|-------|
| 5.1 | Node pulse animation | 4 | `src/tui/widgets/node_box.rs` |
| 5.2 | Edge flow animation | 3 | `src/tui/widgets/edge_line.rs` |
| 5.3 | DAG state serialization | 5 | `src/dag/serialize.rs` |
| 5.4 | Session DAG persistence | 4 | `src/tui/session.rs` |
| 5.5 | Export to Mermaid | 4 | `src/dag/export.rs` |
| 5.6 | Export to JSON | 3 | `src/dag/export.rs` |
| **Total** | | **23** | |

---

## Task 5.1: Node Pulse Animation

**Files:**
- Modify: `src/tui/widgets/node_box.rs`

**Step 1: Write the failing test**

```rust
// In src/tui/widgets/node_box.rs tests

#[test]
fn test_node_animation_state_default() {
    let node = NodeBox::new("msg-001", NodeKind::UserMessage);
    assert_eq!(node.animation_state, AnimationState::Idle);
}

#[test]
fn test_node_animation_state_running() {
    let node = NodeBox::new("msg-001", NodeKind::UserMessage)
        .running(true);
    assert_eq!(node.animation_state, AnimationState::Pulsing);
}

#[test]
fn test_node_pulse_cycle() {
    let mut node = NodeBox::new("msg-001", NodeKind::UserMessage)
        .running(true);

    // Tick animation
    let initial_intensity = node.pulse_intensity();
    node.tick();
    let next_intensity = node.pulse_intensity();

    // Intensity should change
    assert_ne!(initial_intensity, next_intensity);
}

#[test]
fn test_node_animation_completed() {
    let node = NodeBox::new("msg-001", NodeKind::UserMessage)
        .completed(true);
    assert_eq!(node.animation_state, AnimationState::Completed);
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test node_animation --lib
```

Expected: FAIL with "cannot find value `AnimationState`"

**Step 3: Write minimal implementation**

```rust
// In src/tui/widgets/node_box.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationState {
    #[default]
    Idle,
    Pulsing,
    Completed,
    Error,
}

pub struct NodeBox {
    pub task_id: String,
    pub kind: NodeKind,
    pub is_selected: bool,
    pub is_running: bool,
    pub animation_state: AnimationState,
    pulse_frame: u8,
}

impl NodeBox {
    pub fn new(task_id: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            task_id: task_id.into(),
            kind,
            is_selected: false,
            is_running: false,
            animation_state: AnimationState::Idle,
            pulse_frame: 0,
        }
    }

    pub fn running(mut self, running: bool) -> Self {
        self.is_running = running;
        if running {
            self.animation_state = AnimationState::Pulsing;
        }
        self
    }

    pub fn completed(mut self, completed: bool) -> Self {
        if completed {
            self.animation_state = AnimationState::Completed;
        }
        self
    }

    pub fn tick(&mut self) {
        if self.animation_state == AnimationState::Pulsing {
            self.pulse_frame = (self.pulse_frame + 1) % 8;
        }
    }

    pub fn pulse_intensity(&self) -> u8 {
        if self.animation_state != AnimationState::Pulsing {
            return 0;
        }
        // Sine-wave-like intensity: 0-255
        let intensities = [100, 150, 200, 255, 200, 150, 100, 50];
        intensities[self.pulse_frame as usize]
    }

    fn border_style(&self) -> Style {
        let base = Style::default().fg(self.kind.color());

        match self.animation_state {
            AnimationState::Idle => {
                if self.is_selected {
                    base.add_modifier(Modifier::BOLD).fg(Color::White)
                } else {
                    base
                }
            }
            AnimationState::Pulsing => {
                let intensity = self.pulse_intensity();
                let color = Color::Rgb(intensity, intensity, 50);
                base.fg(color).add_modifier(Modifier::BOLD)
            }
            AnimationState::Completed => {
                base.fg(Color::Green).add_modifier(Modifier::BOLD)
            }
            AnimationState::Error => {
                base.fg(Color::Red).add_modifier(Modifier::BOLD)
            }
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test node_animation --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/node_box.rs
git commit -m "feat(tui): add pulse animation to NodeBox

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 5.2: Edge Flow Animation

**Files:**
- Modify: `src/tui/widgets/edge_line.rs`

**Step 1: Write the failing test**

```rust
// In src/tui/widgets/edge_line.rs tests

#[test]
fn test_edge_animation_state() {
    let edge = EdgeLine::new((0, 0), (10, 10));
    assert!(!edge.is_active);
}

#[test]
fn test_edge_active_animation() {
    let edge = EdgeLine::new((0, 0), (10, 10)).active(true);
    assert!(edge.is_active);
}

#[test]
fn test_edge_flow_position() {
    let mut edge = EdgeLine::new((0, 0), (10, 0)).active(true);

    let initial_pos = edge.flow_position();
    edge.tick();
    let next_pos = edge.flow_position();

    assert_ne!(initial_pos, next_pos);
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test edge_animation --lib
```

Expected: FAIL with field not found

**Step 3: Write minimal implementation**

```rust
// In src/tui/widgets/edge_line.rs

pub struct EdgeLine {
    pub from: (u16, u16),
    pub to: (u16, u16),
    pub style: Style,
    pub is_active: bool,
    flow_frame: u8,
}

impl EdgeLine {
    pub fn new(from: (u16, u16), to: (u16, u16)) -> Self {
        Self {
            from,
            to,
            style: Style::default().fg(Color::DarkGray),
            is_active: false,
            flow_frame: 0,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.is_active = active;
        if active {
            self.style = self.style.fg(Color::Cyan);
        }
        self
    }

    pub fn tick(&mut self) {
        if self.is_active {
            self.flow_frame = (self.flow_frame + 1) % 4;
        }
    }

    pub fn flow_position(&self) -> u8 {
        self.flow_frame
    }

    fn flow_char(&self) -> char {
        if self.is_active {
            let chars = ['·', '•', '●', '•'];
            chars[self.flow_frame as usize]
        } else if self.is_horizontal() {
            '─'
        } else if self.is_vertical() {
            '│'
        } else {
            '↘'
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test edge_animation --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/edge_line.rs
git commit -m "feat(tui): add flow animation to EdgeLine

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 5.3: DAG State Serialization

**Files:**
- Create: `src/dag/serialize.rs`
- Modify: `src/dag/mod.rs` (export)

**Step 1: Write the failing test**

```rust
// src/dag/serialize.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_flow_graph() {
        let mut graph = FlowGraph::new();
        graph.add_node("msg-001");
        graph.add_node("msg-002");
        graph.add_edge("msg-001", "msg-002").unwrap();

        let serialized = graph.to_serializable();
        assert_eq!(serialized.nodes.len(), 2);
        assert_eq!(serialized.edges.len(), 1);
    }

    #[test]
    fn test_deserialize_flow_graph() {
        let serialized = SerializableGraph {
            nodes: vec!["msg-001".into(), "msg-002".into()],
            edges: vec![("msg-001".into(), "msg-002".into())],
        };

        let graph = FlowGraph::from_serializable(&serialized);
        assert_eq!(graph.node_count(), 2);
        assert!(graph.has_edge("msg-001", "msg-002"));
    }

    #[test]
    fn test_round_trip_serialization() {
        let mut graph = FlowGraph::new();
        graph.add_node("a");
        graph.add_node("b");
        graph.add_node("c");
        graph.add_edge("a", "b").unwrap();
        graph.add_edge("b", "c").unwrap();

        let serialized = graph.to_serializable();
        let json = serde_json::to_string(&serialized).unwrap();
        let deserialized: SerializableGraph = serde_json::from_str(&json).unwrap();
        let restored = FlowGraph::from_serializable(&deserialized);

        assert_eq!(restored.node_count(), 3);
        assert!(restored.has_edge("a", "b"));
        assert!(restored.has_edge("b", "c"));
    }

    #[test]
    fn test_serialize_empty_graph() {
        let graph = FlowGraph::new();
        let serialized = graph.to_serializable();
        assert!(serialized.nodes.is_empty());
        assert!(serialized.edges.is_empty());
    }

    #[test]
    fn test_serialize_with_metadata() {
        let mut graph = FlowGraph::new();
        graph.add_node("msg-001");

        let serialized = SerializableGraphWithMeta {
            graph: graph.to_serializable(),
            created_at: chrono::Utc::now(),
            version: "0.9.1".to_string(),
        };

        let json = serde_json::to_string(&serialized).unwrap();
        assert!(json.contains("version"));
        assert!(json.contains("0.9.1"));
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test serialize --lib
```

Expected: FAIL with type not found

**Step 3: Write minimal implementation**

```rust
// src/dag/serialize.rs

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use super::flow_graph::FlowGraph;

/// Serializable representation of a FlowGraph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

/// Serializable graph with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableGraphWithMeta {
    pub graph: SerializableGraph,
    pub created_at: DateTime<Utc>,
    pub version: String,
}

impl FlowGraph {
    /// Convert to serializable format
    pub fn to_serializable(&self) -> SerializableGraph {
        let nodes: Vec<String> = self.nodes()
            .map(|id| id.to_string())
            .collect();

        let edges: Vec<(String, String)> = self.edges()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect();

        SerializableGraph { nodes, edges }
    }

    /// Restore from serializable format
    pub fn from_serializable(data: &SerializableGraph) -> Self {
        let mut graph = Self::new();

        for node in &data.nodes {
            graph.add_node(node);
        }

        for (from, to) in &data.edges {
            let _ = graph.add_edge(from, to);
        }

        graph
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test serialize --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/dag/serialize.rs src/dag/mod.rs
git commit -m "feat(dag): add graph serialization for persistence

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 5.4: Session DAG Persistence

**Files:**
- Modify: `src/tui/session.rs`

**Step 1: Write the failing test**

```rust
// In src/tui/session.rs tests

#[test]
fn test_session_saves_dag_state() {
    let temp = TempDir::new().unwrap();
    let session_path = temp.path().join("test-session.json");

    let mut session = Session::new();
    session.dag_graph = Some(SerializableGraph {
        nodes: vec!["msg-001".into()],
        edges: vec![],
    });

    session.save(&session_path).unwrap();
    assert!(session_path.exists());

    let json = std::fs::read_to_string(&session_path).unwrap();
    assert!(json.contains("msg-001"));
}

#[test]
fn test_session_restores_dag_state() {
    let temp = TempDir::new().unwrap();
    let session_path = temp.path().join("test-session.json");

    // Save session with DAG
    let mut session = Session::new();
    session.dag_graph = Some(SerializableGraph {
        nodes: vec!["msg-001".into(), "msg-002".into()],
        edges: vec![("msg-001".into(), "msg-002".into())],
    });
    session.save(&session_path).unwrap();

    // Restore
    let restored = Session::load(&session_path).unwrap();
    let dag = restored.dag_graph.unwrap();
    assert_eq!(dag.nodes.len(), 2);
    assert_eq!(dag.edges.len(), 1);
}

#[test]
fn test_session_auto_saves_dag() {
    let temp = TempDir::new().unwrap();
    let mut manager = SessionManager::new(temp.path().to_path_buf());

    // Add to DAG
    manager.session.dag_graph = Some(SerializableGraph {
        nodes: vec!["msg-001".into()],
        edges: vec![],
    });

    manager.trigger_auto_save();

    // Verify save occurred
    let sessions: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(!sessions.is_empty());
}

#[test]
fn test_chat_view_persists_dag_on_exit() {
    let temp = TempDir::new().unwrap();
    let mut view = ChatView::new()
        .with_session_dir(temp.path().to_path_buf());

    view.add_message("Hello", Role::User);
    view.add_message("Hi!", Role::Assistant);

    view.save_session();

    // Verify session file contains DAG
    let session_files: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    assert!(!session_files.is_empty());
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test session_dag --lib
```

Expected: FAIL with field not found

**Step 3: Write minimal implementation**

Modify `src/tui/session.rs`:

```rust
use crate::dag::serialize::SerializableGraph;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub open_files: Vec<OpenFile>,
    pub active_file: Option<String>,
    pub scroll_offset: u16,
    pub timestamp: DateTime<Utc>,
    // NEW: DAG state
    pub dag_graph: Option<SerializableGraph>,
    pub chat_messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            open_files: Vec::new(),
            active_file: None,
            scroll_offset: 0,
            timestamp: Utc::now(),
            dag_graph: None,
            chat_messages: Vec::new(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl ChatView {
    pub fn save_session(&self) {
        if let Some(manager) = &self.session_manager {
            let mut session = Session::new();

            // Save DAG state
            if let Some(panel) = &self.dag_panel {
                session.dag_graph = Some(SerializableGraph {
                    nodes: panel.nodes.iter().map(|(id, _)| id.clone()).collect(),
                    edges: panel.edges.clone(),
                });
            }

            // Save messages
            session.chat_messages = self.messages.iter().map(|m| ChatMessage {
                id: m.id.clone(),
                role: format!("{:?}", m.role),
                content: m.content.clone(),
                timestamp: Utc::now(),
            }).collect();

            manager.save(&session);
        }
    }

    pub fn restore_session(&mut self) {
        if let Some(manager) = &self.session_manager {
            if let Some(session) = manager.load_latest() {
                // Restore DAG
                if let Some(graph) = &session.dag_graph {
                    if let Some(panel) = &mut self.dag_panel {
                        panel.nodes.clear();
                        panel.edges.clear();
                        for node_id in &graph.nodes {
                            panel.add_node(node_id, NodeKind::SystemMessage);
                        }
                        for (from, to) in &graph.edges {
                            panel.edges.push((from.clone(), to.clone()));
                        }
                    }
                }

                // Restore messages
                for msg in &session.chat_messages {
                    let role = match msg.role.as_str() {
                        "User" => Role::User,
                        "Assistant" => Role::Assistant,
                        _ => Role::System,
                    };
                    self.messages.push(Message {
                        id: msg.id.clone(),
                        content: msg.content.clone(),
                        role,
                    });
                }
            }
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test session_dag --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/session.rs src/tui/views/chat.rs
git commit -m "feat(session): persist DAG state across chat sessions

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 5.5: Export to Mermaid

**Files:**
- Create: `src/dag/export.rs`

**Step 1: Write the failing test**

```rust
// src/dag/export.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_mermaid_simple() {
        let mut graph = FlowGraph::new();
        graph.add_node("msg-001");
        graph.add_node("msg-002");
        graph.add_edge("msg-001", "msg-002").unwrap();

        let mermaid = export_mermaid(&graph);
        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("msg-001"));
        assert!(mermaid.contains("msg-002"));
        assert!(mermaid.contains("-->"));
    }

    #[test]
    fn test_export_mermaid_empty() {
        let graph = FlowGraph::new();
        let mermaid = export_mermaid(&graph);
        assert!(mermaid.contains("graph TD"));
    }

    #[test]
    fn test_export_mermaid_with_styles() {
        let mut graph = FlowGraph::new();
        graph.add_node_with_kind("user-001", "user");
        graph.add_node_with_kind("assistant-001", "assistant");
        graph.add_edge("user-001", "assistant-001").unwrap();

        let mermaid = export_mermaid_styled(&graph);
        assert!(mermaid.contains("classDef user"));
        assert!(mermaid.contains("classDef assistant"));
    }

    #[test]
    fn test_export_mermaid_escapes_special_chars() {
        let mut graph = FlowGraph::new();
        graph.add_node("msg with spaces");
        graph.add_node("msg-with-dashes");

        let mermaid = export_mermaid(&graph);
        // IDs should be sanitized
        assert!(mermaid.contains("msg_with_spaces") || mermaid.contains("\"msg with spaces\""));
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test export_mermaid --lib
```

Expected: FAIL with function not found

**Step 3: Write minimal implementation**

```rust
// src/dag/export.rs

use super::flow_graph::FlowGraph;

/// Export FlowGraph to Mermaid diagram format
pub fn export_mermaid(graph: &FlowGraph) -> String {
    let mut output = String::from("graph TD\n");

    // Add nodes
    for node_id in graph.nodes() {
        let safe_id = sanitize_mermaid_id(node_id);
        output.push_str(&format!("    {}[\"{}\"]\n", safe_id, node_id));
    }

    // Add edges
    for (from, to) in graph.edges() {
        let safe_from = sanitize_mermaid_id(from);
        let safe_to = sanitize_mermaid_id(to);
        output.push_str(&format!("    {} --> {}\n", safe_from, safe_to));
    }

    output
}

/// Export with Solarized-style classes
pub fn export_mermaid_styled(graph: &FlowGraph) -> String {
    let mut output = export_mermaid(graph);

    // Add style definitions (Tailwind-Solarized compatible)
    output.push_str("\n");
    output.push_str("    classDef user fill:#268bd2,stroke:#073642,color:#fdf6e3\n");
    output.push_str("    classDef assistant fill:#859900,stroke:#073642,color:#fdf6e3\n");
    output.push_str("    classDef tool fill:#b58900,stroke:#073642,color:#fdf6e3\n");
    output.push_str("    classDef system fill:#93a1a1,stroke:#073642,color:#002b36\n");

    // Apply classes based on node kind (if available)
    for node_id in graph.nodes() {
        let safe_id = sanitize_mermaid_id(node_id);
        let class = if node_id.starts_with("user") {
            "user"
        } else if node_id.starts_with("assistant") || node_id.starts_with("msg") && node_id.ends_with("a") {
            "assistant"
        } else if node_id.starts_with("tool") {
            "tool"
        } else {
            "system"
        };
        output.push_str(&format!("    class {} {}\n", safe_id, class));
    }

    output
}

/// Sanitize ID for Mermaid (no spaces, special chars)
fn sanitize_mermaid_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test export_mermaid --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/dag/export.rs src/dag/mod.rs
git commit -m "feat(dag): add Mermaid diagram export with Solarized styles

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 5.6: Export to JSON

**Files:**
- Modify: `src/dag/export.rs`

**Step 1: Write the failing test**

```rust
// In src/dag/export.rs tests

#[test]
fn test_export_json() {
    let mut graph = FlowGraph::new();
    graph.add_node("msg-001");
    graph.add_node("msg-002");
    graph.add_edge("msg-001", "msg-002").unwrap();

    let json = export_json(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed["nodes"].is_array());
    assert!(parsed["edges"].is_array());
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 2);
}

#[test]
fn test_export_json_with_metadata() {
    let mut graph = FlowGraph::new();
    graph.add_node("msg-001");

    let json = export_json_with_meta(&graph, "test-session");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed["metadata"]["session_id"].is_string());
    assert!(parsed["metadata"]["exported_at"].is_string());
    assert!(parsed["metadata"]["version"].as_str().unwrap().contains("0.9"));
}

#[test]
fn test_export_json_pretty() {
    let mut graph = FlowGraph::new();
    graph.add_node("msg-001");

    let json = export_json_pretty(&graph);
    assert!(json.contains('\n')); // Pretty printed has newlines
    assert!(json.contains("  ")); // And indentation
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test export_json --lib
```

Expected: FAIL with function not found

**Step 3: Write minimal implementation**

```rust
// Add to src/dag/export.rs

use chrono::Utc;
use serde_json::json;

/// Export FlowGraph to JSON format
pub fn export_json(graph: &FlowGraph) -> String {
    let serializable = graph.to_serializable();
    serde_json::to_string(&serializable).unwrap_or_default()
}

/// Export FlowGraph to pretty-printed JSON
pub fn export_json_pretty(graph: &FlowGraph) -> String {
    let serializable = graph.to_serializable();
    serde_json::to_string_pretty(&serializable).unwrap_or_default()
}

/// Export with metadata (session info, timestamp, version)
pub fn export_json_with_meta(graph: &FlowGraph, session_id: &str) -> String {
    let serializable = graph.to_serializable();

    let export = json!({
        "metadata": {
            "session_id": session_id,
            "exported_at": Utc::now().to_rfc3339(),
            "version": env!("CARGO_PKG_VERSION"),
            "format": "nika-dag-v1"
        },
        "nodes": serializable.nodes,
        "edges": serializable.edges
    });

    serde_json::to_string_pretty(&export).unwrap_or_default()
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test export_json --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/dag/export.rs
git commit -m "feat(dag): add JSON export with metadata

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## 🔌 WIRING CHECKPOINT 5: Session ↔ DAG State Restore

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  WIRING CHECKPOINT 5: Session ↔ DAG State Restore                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Verify these connections are working:                                        ║
║                                                                               ║
║  1. ChatView.save_session() persists DAG to .nika/sessions/                   ║
║  2. ChatView.restore_session() loads DAG on startup                           ║
║  3. Session contains both messages AND DAG graph                              ║
║  4. Export functions produce valid Mermaid/JSON                               ║
║  5. Node animations tick correctly                                            ║
║  6. Full round-trip: exit → restart → DAG restored                            ║
║                                                                               ║
║  Run: cargo test wiring_checkpoint_5 --lib                                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Wiring Test:**

```rust
// tests/wiring_checkpoint_5.rs

#[test]
fn wiring_checkpoint_5_session_dag_restore() {
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();

    // Create view and add messages
    let mut view = ChatView::new()
        .with_session_dir(temp.path().to_path_buf());

    view.add_message("First message", Role::User);
    view.add_message("Response", Role::Assistant);
    view.add_message("Follow-up @1", Role::User);

    // Save session
    view.save_session();

    // Create new view and restore
    let mut restored_view = ChatView::new()
        .with_session_dir(temp.path().to_path_buf());

    restored_view.restore_session();

    // Verify DAG was restored
    let panel = restored_view.dag_panel.as_ref().unwrap();
    assert_eq!(panel.nodes.len(), 3);
    assert!(!panel.edges.is_empty());

    // Verify messages were restored
    assert_eq!(restored_view.messages.len(), 3);
}

#[test]
fn wiring_checkpoint_5_export_valid_mermaid() {
    let mut graph = FlowGraph::new();
    graph.add_node("user-001");
    graph.add_node("assistant-001");
    graph.add_edge("user-001", "assistant-001").unwrap();

    let mermaid = export_mermaid_styled(&graph);

    // Should be valid Mermaid syntax
    assert!(mermaid.starts_with("graph TD"));
    assert!(mermaid.contains("-->"));
    assert!(mermaid.contains("classDef"));
}
```

---

## 🧪 LIVE TEST: Full Polish Verification

After completing all Phase 5 tasks, run these live tests:

```bash
# Test 1: Session persistence
cargo run -- chat
# Type messages, then exit with Ctrl+C
cargo run -- chat
# Verify messages and DAG are restored

# Test 2: Export
cargo run -- export dag --format mermaid > dag.md
cargo run -- export dag --format json > dag.json
# Verify files are valid

# Test 3: Visual animations
cargo run -- chat
# Send a message, observe:
# - Node pulses while AI is thinking
# - Edge flows during data transfer
# - Node turns green when complete
```

**Visual Verification Checklist:**

- [ ] Running nodes pulse with yellow animation
- [ ] Completed nodes show green border
- [ ] Edges animate during data flow
- [ ] Session restores DAG after restart
- [ ] Mermaid export produces valid diagram
- [ ] JSON export contains all nodes/edges

---

## Summary

| Task | Description | Tests | Status |
|------|-------------|-------|--------|
| 5.1 | Node pulse animation | 4 | ⬜ |
| 5.2 | Edge flow animation | 3 | ⬜ |
| 5.3 | DAG state serialization | 5 | ⬜ |
| 5.4 | Session DAG persistence | 4 | ⬜ |
| 5.5 | Export to Mermaid | 4 | ⬜ |
| 5.6 | Export to JSON | 3 | ⬜ |
| **Total** | | **23** | |

---

## 🎯 FINAL: /nika-deep-verify

After completing all 5 phases, run the comprehensive verification:

```bash
/nika-deep-verify
```

This launches 6 parallel agents to verify:
1. Spec-code alignment
2. Rust conventions
3. Documentation sync
4. Logic consistency
5. Claude structure
6. Test coverage

---

## Success Criteria (v0.9.1)

- [ ] All 160+ new tests pass
- [ ] Existing 1,902 tests unchanged
- [ ] 5 WIRING checkpoints verified
- [ ] 5 live tests pass
- [ ] Zero clippy warnings
- [ ] `/nika-deep-verify` passes (6 agents)
- [ ] Session persistence works across restarts
- [ ] Mermaid/JSON exports are valid

---

## References

- [Phase 4: DAG Panel](./Phase-4-DagPanel.md)
- [v0.8.0 Session System](../../../tools/nika/CLAUDE.md#session-persistence)
- [Thread Safety Architecture](./2026-02-24-thread-safety-architecture.md)
- [Master Plan](./2026-02-24-v091-master-plan.md)
