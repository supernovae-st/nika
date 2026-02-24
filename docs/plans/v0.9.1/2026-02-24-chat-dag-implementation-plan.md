# Chat as Workflow DAG — Implementation Plan

**Date:** 2026-02-24
**Version:** v0.9.0 Target
**Design Document:** [2026-02-24-chat-as-workflow-dag.md](./2026-02-24-chat-as-workflow-dag.md)

---

## Overview

Implementation plan for unifying Chat TUI with Workflow DAG system. Each chat message becomes a Task with full DataStore, EventLog, and binding support.

---

## Phase 1: Infrastructure (No UI Changes)

**Goal:** Wire DataStore and EventLog into chat execution without changing UX.

### Task 1.1: Create ChatWorkflow Struct

**File:** `src/tui/chat_workflow.rs` (NEW)

```rust
use crate::ast::{Workflow, Task, TaskAction};
use crate::store::DataStore;
use crate::event::EventLog;
use crate::dag::FlowGraph;

pub struct ChatWorkflow {
    /// Incremental workflow being built
    pub workflow: Workflow,
    /// DAG representation
    pub dag: FlowGraph,
    /// Result storage
    pub store: DataStore,
    /// Event log for observability
    pub log: EventLog,
    /// Message counter for ID generation
    pub message_counter: u32,
}

impl ChatWorkflow {
    pub fn new(session_id: &str) -> Self {
        Self {
            workflow: Workflow {
                schema: "nika/workflow@0.5".into(),
                workflow: format!("chat-session-{}", session_id),
                description: Some("Interactive chat session".into()),
                tasks: Vec::new(),
                flows: Vec::new(),
                mcp: None,
            },
            dag: FlowGraph::new(),
            store: DataStore::new(),
            log: EventLog::new(),
            message_counter: 0,
        }
    }

    /// Generate next message ID (msg-001, msg-002, ...)
    pub fn next_message_id(&mut self) -> String {
        self.message_counter += 1;
        format!("msg-{:03}", self.message_counter)
    }

    /// Add a task to the workflow and DAG
    pub fn add_task(&mut self, task: Task) {
        self.dag.add_node(&task);
        self.workflow.tasks.push(task);
    }

    /// Add a flow (edge) between tasks
    pub fn add_flow(&mut self, source: &str, target: &str) {
        use crate::ast::Flow;
        self.workflow.flows.push(Flow {
            source: source.into(),
            target: target.into(),
        });
        self.dag.add_edge(source, target);
    }
}
```

**Tests:** `src/tui/chat_workflow.rs` (10+ unit tests)
- `test_new_creates_empty_workflow`
- `test_next_message_id_increments`
- `test_add_task_updates_dag`
- `test_add_flow_creates_edge`

### Task 1.2: Create ChatTask Builder

**File:** `src/tui/chat_task.rs` (NEW)

```rust
use crate::ast::{Task, TaskAction, InferParams, WiringSpec};

pub struct ChatTaskBuilder {
    id: String,
    action: TaskAction,
    use_wiring: Option<WiringSpec>,
    depends_on: Vec<String>,
}

impl ChatTaskBuilder {
    pub fn new(id: String, action: TaskAction) -> Self {
        Self {
            id,
            action,
            use_wiring: None,
            depends_on: Vec::new(),
        }
    }

    /// Create infer task from chat message
    pub fn from_message(id: String, prompt: &str) -> Self {
        Self::new(id, TaskAction::Infer(InferParams {
            prompt: prompt.to_string(),
            model: None, // Use default
            ..Default::default()
        }))
    }

    pub fn depends_on(mut self, task_id: &str) -> Self {
        self.depends_on.push(task_id.to_string());
        self
    }

    pub fn with_wiring(mut self, wiring: WiringSpec) -> Self {
        self.use_wiring = Some(wiring);
        self
    }

    pub fn build(self) -> Task {
        Task {
            id: self.id,
            action: self.action,
            use_wiring: self.use_wiring,
            output: None,
            condition: None,
            for_each: None,
            decompose: None,
        }
    }
}
```

### Task 1.3: Wire DataStore into ChatAgent

**File:** `src/tui/chat_agent.rs` (MODIFY)

```rust
// Before (current)
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    // ...
}

// After (add ChatWorkflow)
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    workflow: ChatWorkflow,  // NEW
    // ...
}

impl ChatAgent {
    pub fn new(provider: RigProvider) -> Self {
        Self {
            provider,
            history: Vec::new(),
            workflow: ChatWorkflow::new(&Uuid::new_v4().to_string()),
        }
    }

    pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
        // 1. Create task
        let task_id = self.workflow.next_message_id();
        let task = ChatTaskBuilder::from_message(task_id.clone(), prompt)
            .build();

        // 2. Add to workflow
        self.workflow.add_task(task.clone());

        // 3. Emit TaskStarted event
        self.workflow.log.emit(EventKind::TaskStarted {
            task_id: task_id.clone().into(),
            action_type: "infer".into(),
        });

        // 4. Execute via provider (existing code)
        let result = self.provider.infer(prompt, None).await?;

        // 5. Store result in DataStore
        self.workflow.store.insert(&task_id, serde_json::json!({
            "output": result.clone(),
            "prompt": prompt,
        }));

        // 6. Emit TaskCompleted event
        self.workflow.log.emit(EventKind::TaskCompleted {
            task_id: task_id.into(),
            duration_ms: 0, // TODO: track actual duration
        });

        // 7. Update history (existing code)
        self.history.push(ChatMessage::user(prompt));
        self.history.push(ChatMessage::assistant(&result));

        Ok(result)
    }
}
```

### Task 1.4: Export EventLog

**File:** `src/tui/chat_agent.rs` (MODIFY)

```rust
impl ChatAgent {
    /// Export session as NDJSON trace
    pub fn export_trace(&self, path: &Path) -> Result<(), NikaError> {
        use crate::event::TraceWriter;
        let writer = TraceWriter::new(path)?;
        for event in self.workflow.log.events() {
            writer.write_event(event)?;
        }
        Ok(())
    }

    /// Get all events (for DAG panel)
    pub fn events(&self) -> &[Event] {
        self.workflow.log.events()
    }
}
```

### Verification Phase 1

```bash
# 1. Run tests
cargo test chat_workflow
cargo test chat_task

# 2. Verify DataStore integration
cargo run -- chat
# Send a message, then check:
# - EventLog has TaskStarted + TaskCompleted events
# - DataStore has message result

# 3. Export trace
# After chat session, verify .nika/traces/ has NDJSON file
```

---

## Phase 2: Binding System

**Goal:** Implement @mention parsing and binding resolution.

### Task 2.1: Mention Parser

**File:** `src/tui/mention_parser.rs` (NEW)

```rust
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    /// Match @1, @2, @last, @prev, @all, @msg-001
    static ref MENTION_RE: Regex = Regex::new(
        r"@((\d+)|last|prev|all|msg-\d{3})"
    ).unwrap();
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mention {
    /// @1, @2, etc. (1-indexed)
    Number(u32),
    /// @last - last message
    Last,
    /// @prev - previous message (same as @last in sequential)
    Prev,
    /// @all - all previous messages
    All,
    /// @msg-001 - explicit ID
    Explicit(String),
}

impl Mention {
    /// Resolve to actual task ID given message count
    pub fn resolve(&self, message_count: u32) -> Vec<String> {
        match self {
            Mention::Number(n) => vec![format!("msg-{:03}", n)],
            Mention::Last => vec![format!("msg-{:03}", message_count)],
            Mention::Prev => vec![format!("msg-{:03}", message_count)],
            Mention::All => (1..=message_count)
                .map(|n| format!("msg-{:03}", n))
                .collect(),
            Mention::Explicit(id) => vec![id.clone()],
        }
    }
}

/// Parse all @mentions from a message
pub fn parse_mentions(text: &str) -> Vec<Mention> {
    MENTION_RE.captures_iter(text)
        .filter_map(|cap| {
            let m = cap.get(1)?.as_str();
            Some(match m {
                "last" => Mention::Last,
                "prev" => Mention::Prev,
                "all" => Mention::All,
                s if s.starts_with("msg-") => Mention::Explicit(s.into()),
                s => Mention::Number(s.parse().ok()?),
            })
        })
        .collect()
}

/// Check if message starts with // (parallel prefix)
pub fn is_parallel(text: &str) -> bool {
    text.trim_start().starts_with("//")
}

/// Strip prefix commands from message
pub fn strip_prefix(text: &str) -> &str {
    let text = text.trim_start();
    if text.starts_with("//") {
        text[2..].trim_start()
    } else if text.starts_with("/infer") {
        text[6..].trim_start()
    } else if text.starts_with("/exec") {
        text[5..].trim_start()
    } else if text.starts_with("/fetch") {
        text[6..].trim_start()
    } else if text.starts_with("/invoke") {
        text[7..].trim_start()
    } else if text.starts_with("/agent") {
        text[6..].trim_start()
    } else {
        text
    }
}
```

**Tests:** `src/tui/mention_parser.rs` (15+ unit tests)
- `test_parse_numeric_mention`
- `test_parse_last_mention`
- `test_parse_multiple_mentions`
- `test_resolve_to_task_ids`
- `test_is_parallel_detection`

### Task 2.2: MentionToBinding Converter

**File:** `src/tui/mention_binding.rs` (NEW)

```rust
use crate::ast::WiringSpec;
use crate::binding::UseEntry;
use super::mention_parser::{Mention, parse_mentions, is_parallel};

/// Convert @mentions to WiringSpec
pub fn mentions_to_wiring(
    text: &str,
    message_count: u32,
    prev_task_id: Option<&str>,
) -> WiringSpec {
    let mentions = parse_mentions(text);

    // If message is parallel (//), no dependencies
    if is_parallel(text) {
        return WiringSpec::default();
    }

    // If explicit @mentions, use those
    if !mentions.is_empty() {
        let entries: Vec<UseEntry> = mentions
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                m.resolve(message_count)
                    .into_iter()
                    .map(move |id| UseEntry {
                        alias: format!("m{}", i + 1),
                        path: format!("{}.output", id).parse().unwrap(),
                        lazy: false,
                        default: None,
                    })
            })
            .collect();

        return WiringSpec { entries };
    }

    // Default: depend on previous message (sequential)
    if let Some(prev_id) = prev_task_id {
        WiringSpec {
            entries: vec![UseEntry {
                alias: "prev".into(),
                path: format!("{}.output", prev_id).parse().unwrap(),
                lazy: false,
                default: None,
            }],
        }
    } else {
        WiringSpec::default()
    }
}
```

### Task 2.3: Integrate Bindings into ChatAgent

**File:** `src/tui/chat_agent.rs` (MODIFY)

```rust
impl ChatAgent {
    pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
        // 1. Parse @mentions and determine dependencies
        let prev_task_id = self.workflow.workflow.tasks.last()
            .map(|t| t.id.as_str());
        let wiring = mentions_to_wiring(
            prompt,
            self.workflow.message_counter,
            prev_task_id,
        );

        // 2. Create task with wiring
        let task_id = self.workflow.next_message_id();
        let task = ChatTaskBuilder::from_message(task_id.clone(), prompt)
            .with_wiring(wiring.clone())
            .build();

        // 3. Add flows for dependencies
        for entry in &wiring.entries {
            let source_id = entry.path.task_id();
            self.workflow.add_flow(&source_id, &task_id);
        }

        // 4. Add to workflow (existing code)
        self.workflow.add_task(task);

        // ... rest of execution
    }
}
```

### Verification Phase 2

```bash
# 1. Run tests
cargo test mention_parser
cargo test mention_binding

# 2. Integration test
cargo run -- chat
> "Hello"                    # msg-001
> "Expand on that"           # msg-002 depends on msg-001
> // "Independent task"      # msg-003 no dependencies
> "Combine @1 and @3"        # msg-004 depends on msg-001 and msg-003
```

---

## Phase 3: DAG Panel

**Goal:** Add live DAG visualization sidebar to chat view.

### Task 3.1: Create ChatDagPanel Widget

**File:** `src/tui/widgets/chat_dag_panel.rs` (NEW)

```rust
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Widget},
};
use crate::dag::FlowGraph;
use super::dag_node_box::{NodeBox, NodeBoxMode};

pub struct ChatDagPanel<'a> {
    /// DAG to render
    dag: &'a FlowGraph,
    /// Currently running task ID
    running_task: Option<&'a str>,
    /// Width mode
    expanded: bool,
}

impl<'a> ChatDagPanel<'a> {
    pub fn new(dag: &'a FlowGraph) -> Self {
        Self {
            dag,
            running_task: None,
            expanded: false,
        }
    }

    pub fn running(mut self, task_id: Option<&'a str>) -> Self {
        self.running_task = task_id;
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}

impl Widget for ChatDagPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Border
        let block = Block::default()
            .title(" DAG Live ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(area);
        block.render(area, buf);

        // Render nodes vertically
        let node_height = if self.expanded { 7 } else { 3 };
        let mut y = inner.y;

        for (i, node_id) in self.dag.topological_order().iter().enumerate() {
            if y + node_height > inner.bottom() {
                break; // No more space
            }

            let node_area = Rect::new(inner.x, y, inner.width, node_height);
            let is_running = self.running_task == Some(node_id.as_str());

            // Render node box
            NodeBox::new(node_id)
                .mode(if self.expanded {
                    NodeBoxMode::Full
                } else {
                    NodeBoxMode::Expanded
                })
                .running(is_running)
                .render(node_area, buf);

            // Render edge to next node
            if i < self.dag.node_count() - 1 {
                y += node_height;
                if y < inner.bottom() {
                    let edge_char = if is_running { '▼' } else { '│' };
                    buf.set_string(
                        inner.x + inner.width / 2,
                        y,
                        edge_char.to_string(),
                        Style::default().fg(Color::DarkGray),
                    );
                    y += 1;
                }
            } else {
                y += node_height;
            }
        }

        // Footer: stats
        let stats = format!(
            " {} tasks {} layers ",
            self.dag.node_count(),
            self.dag.layer_count(),
        );
        buf.set_string(
            inner.right() - stats.len() as u16 - 1,
            area.bottom() - 1,
            stats,
            Style::default().fg(Color::DarkGray),
        );
    }
}
```

### Task 3.2: Add Sidebar Layout to Chat View

**File:** `src/tui/views/chat.rs` (MODIFY)

```rust
impl ChatView {
    fn render_inner(&self, area: Rect, buf: &mut Buffer) {
        // Split into chat (left) and DAG (right)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(40),              // Chat (flexible)
                Constraint::Length(self.dag_width), // DAG panel (fixed)
            ])
            .split(area);

        // Render chat messages (existing code)
        self.render_chat(chunks[0], buf);

        // Render DAG panel (NEW)
        ChatDagPanel::new(&self.agent.workflow.dag)
            .running(self.running_task.as_deref())
            .expanded(self.dag_expanded)
            .render(chunks[1], buf);
    }
}
```

### Task 3.3: Wire Live Updates

**File:** `src/tui/views/chat.rs` (MODIFY)

```rust
impl ChatView {
    /// Called when a task starts
    fn on_task_started(&mut self, task_id: &str) {
        self.running_task = Some(task_id.to_string());
        // Trigger re-render
    }

    /// Called when a task completes
    fn on_task_completed(&mut self, task_id: &str) {
        self.running_task = None;
        // Trigger re-render
    }
}
```

### Task 3.4: Node Click → Scroll to Message

**File:** `src/tui/views/chat.rs` (MODIFY)

```rust
impl ChatView {
    fn handle_dag_click(&mut self, x: u16, y: u16) {
        // Determine which node was clicked
        if let Some(task_id) = self.get_node_at(x, y) {
            // Find message index
            if let Some(msg_idx) = self.get_message_index(&task_id) {
                self.scroll_to_message(msg_idx);
            }
        }
    }
}
```

### Verification Phase 3

```bash
# 1. Visual test
cargo run -- chat
# Verify sidebar appears on right
# Verify nodes appear as messages are sent
# Verify running spinner while task executes

# 2. Keyboard test
# Press Ctrl+D to toggle DAG width
# Press Ctrl+E to expand/collapse nodes
```

---

## Phase 4: Enhanced NodeBox

**Goal:** Remove Minimal mode, enhance Expanded to Full with more info.

### Task 4.1: Add Full Mode to NodeBox

**File:** `src/tui/widgets/dag_node_box.rs` (MODIFY)

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeBoxMode {
    /// Compact 3-line view (DEPRECATED - remove)
    // Minimal,
    /// Standard 5-line view
    Expanded,
    /// Full 7-line view with tokens, output, bindings
    Full,
}

pub struct NodeBox<'a> {
    task_id: &'a str,
    mode: NodeBoxMode,
    // NEW fields for Full mode
    tokens_in: Option<u32>,
    tokens_out: Option<u32>,
    output_preview: Option<&'a str>,
    bindings: Vec<&'a str>,
    duration_ms: Option<u64>,
}

impl<'a> NodeBox<'a> {
    pub fn tokens(mut self, input: u32, output: u32) -> Self {
        self.tokens_in = Some(input);
        self.tokens_out = Some(output);
        self
    }

    pub fn output(mut self, preview: &'a str) -> Self {
        self.output_preview = Some(preview);
        self
    }

    pub fn bindings(mut self, bindings: Vec<&'a str>) -> Self {
        self.bindings = bindings;
        self
    }

    pub fn duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }
}

impl Widget for NodeBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            NodeBoxMode::Expanded => self.render_expanded(area, buf),
            NodeBoxMode::Full => self.render_full(area, buf),
        }
    }
}

impl NodeBox<'_> {
    fn render_full(&self, area: Rect, buf: &mut Buffer) {
        // Line 1: Icon + ID + Duration + Status
        // Line 2: ────────────────────────────
        // Line 3: Model + Tokens
        // Line 4: ────────────────────────────
        // Line 5: Prompt preview
        // Line 6: ────────────────────────────
        // Line 7: Output preview
        // Line 8: ────────────────────────────
        // Line 9: Bindings

        let lines = vec![
            self.format_header(),
            "─".repeat(area.width as usize - 2),
            self.format_model_tokens(),
            "─".repeat(area.width as usize - 2),
            self.format_prompt(),
            "─".repeat(area.width as usize - 2),
            self.format_output(),
            "─".repeat(area.width as usize - 2),
            self.format_bindings(),
        ];

        // Render with box borders
        // ...
    }

    fn format_header(&self) -> String {
        let icon = self.get_verb_icon();
        let duration = self.duration_ms
            .map(|ms| format!("{:.1}s", ms as f64 / 1000.0))
            .unwrap_or_default();
        let status = self.get_status_badge();
        format!("{} {}  {:>8}  {}", icon, self.task_id, duration, status)
    }

    fn format_model_tokens(&self) -> String {
        let model = "🧠 claude-sonnet-4";
        let tokens = match (self.tokens_in, self.tokens_out) {
            (Some(i), Some(o)) => format!("📊 {}→{}", format_tokens(i), format_tokens(o)),
            _ => String::new(),
        };
        format!("{}  {}", model, tokens)
    }

    fn format_output(&self) -> String {
        self.output_preview
            .map(|s| format!("📤 \"{}\"", truncate(s, 50)))
            .unwrap_or_default()
    }

    fn format_bindings(&self) -> String {
        if self.bindings.is_empty() {
            return String::new();
        }
        format!("🔗 use: {}", self.bindings.join(", "))
    }
}

fn format_tokens(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
```

### Task 4.2: Remove Minimal Mode

**Files to modify:**
- `src/tui/widgets/dag_node_box.rs` - Remove Minimal variant
- `src/tui/widgets/dag_ascii.rs` - Update to use Expanded/Full only
- Any tests using NodeBoxMode::Minimal

### Verification Phase 4

```bash
# 1. Run tests
cargo test dag_node_box

# 2. Visual test
cargo run -- chat
# Send messages and verify:
# - Tokens show in Full mode
# - Output preview shows result
# - Bindings display @mentions
```

---

## Phase 5: Polish

**Goal:** Animations, shortcuts, persistence, export.

### Task 5.1: Animations

**File:** `src/tui/widgets/dag_node_box.rs` (MODIFY)

```rust
impl NodeBox<'_> {
    /// Get animated border style based on status
    fn get_border_style(&self, frame: u64) -> Style {
        match self.status {
            TaskStatus::Pending => Style::default().fg(Color::DarkGray),
            TaskStatus::Running => {
                // Pulse animation
                let intensity = ((frame % 20) as f64 / 20.0 * std::f64::consts::PI).sin();
                let color = if intensity > 0.5 { Color::Yellow } else { Color::LightYellow };
                Style::default().fg(color)
            }
            TaskStatus::Completed => Style::default().fg(Color::Green),
            TaskStatus::Failed => {
                // Shake animation (handled in render)
                Style::default().fg(Color::Red)
            }
        }
    }
}
```

### Task 5.2: Keyboard Shortcuts

**File:** `src/tui/views/chat.rs` (MODIFY)

```rust
impl ChatView {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match (key.modifiers, key.code) {
            // Existing shortcuts...

            // NEW: DAG panel shortcuts
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.dag_width = if self.dag_width == 24 { 40 } else { 24 };
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
                self.dag_expanded = !self.dag_expanded;
                None
            }

            _ => None,
        }
    }
}
```

### Task 5.3: Session Persistence for DAG State

**File:** `src/tui/session.rs` (MODIFY)

```rust
#[derive(Serialize, Deserialize)]
pub struct ChatSession {
    // Existing fields...

    /// DAG state for restoration
    pub dag_state: Option<ChatDagState>,
}

#[derive(Serialize, Deserialize)]
pub struct ChatDagState {
    /// Task IDs in order
    pub tasks: Vec<String>,
    /// Edges (source, target)
    pub edges: Vec<(String, String)>,
    /// Results per task
    pub results: HashMap<String, serde_json::Value>,
}
```

### Task 5.4: Export to YAML

**File:** `src/tui/chat_agent.rs` (MODIFY)

```rust
impl ChatAgent {
    /// Export chat session as .nika.yaml workflow
    pub fn export_yaml(&self, path: &Path) -> Result<(), NikaError> {
        let yaml = serde_yaml::to_string(&self.workflow.workflow)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }
}
```

### Verification Phase 5

```bash
# 1. Animation test
cargo run -- chat
# Verify spinning badge on running tasks
# Verify green glow on completion

# 2. Shortcut test
# Press Ctrl+D, verify width changes
# Press Ctrl+E, verify nodes expand/collapse

# 3. Persistence test
# Exit chat, restart, verify DAG restored

# 4. Export test
cargo run -- chat
> "Hello"
> "Continue"
# Export as YAML, verify valid .nika.yaml
```

---

## Test Coverage

| Module | Tests Required | Priority |
|--------|---------------|----------|
| `chat_workflow.rs` | 10 | P0 |
| `chat_task.rs` | 8 | P0 |
| `mention_parser.rs` | 15 | P0 |
| `mention_binding.rs` | 10 | P0 |
| `chat_dag_panel.rs` | 8 | P1 |
| `dag_node_box.rs` (Full mode) | 12 | P1 |
| Integration tests | 10 | P1 |

**Total new tests:** ~73

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| DataStore performance with many messages | Limit to 1000 messages per session |
| DAG panel too wide | Min width 24, max 60, responsive |
| Complex mention parsing | Extensive regex tests, fallback to sequential |
| Session persistence corruption | Atomic writes, backup on load |

---

## Success Metrics

1. **All tests pass:** 100% of new tests green
2. **No regressions:** Existing 1,902 tests pass
3. **Performance:** <50ms frame time with 100 messages
4. **Coverage:** 80%+ on new code
5. **User feedback:** Chat feels same, DAG adds value

---

## Timeline Summary

| Phase | Focus | Deliverables |
|-------|-------|--------------|
| Phase 1 | Infrastructure | ChatWorkflow, DataStore wiring, EventLog |
| Phase 2 | Bindings | @mention parser, MentionToBinding |
| Phase 3 | DAG Panel | ChatDagPanel widget, sidebar layout |
| Phase 4 | NodeBox | Full mode, remove Minimal |
| Phase 5 | Polish | Animations, shortcuts, persistence |

---

## References

- Design Document: [2026-02-24-chat-as-workflow-dag.md](./2026-02-24-chat-as-workflow-dag.md)
- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First Workflow Definition
- `src/tui/widgets/dag_node_box.rs` — Current NodeBox
- `src/tui/widgets/dag_ascii.rs` — Current DAG rendering
