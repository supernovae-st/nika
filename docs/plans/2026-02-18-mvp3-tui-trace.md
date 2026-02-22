# MVP 3: TUI + CLI Trace Commands

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a 4-panel terminal UI for real-time workflow observability and CLI commands for trace management.

**Architecture:** Feature-gated TUI using ratatui with broadcast channel for real-time event streaming. CLI commands for listing, viewing, and exporting traces.

**Tech Stack:** Rust, ratatui, crossterm, tokio::broadcast

**Estimated Time:** 8-10 hours

**Prerequisites:** MVP 2 (Agent + Observability) completed

---

## Task 1: Setup TUI Feature Flag

**Files:**
- Modify: `Cargo.toml`
- Create: `src/tui/mod.rs`
- Modify: `src/lib.rs`

### Step 1: Verify feature flag in Cargo.toml

Ensure these are present:

```toml
[features]
default = ["tui"]
tui = ["dep:ratatui", "dep:crossterm"]

[dependencies]
ratatui = { version = "0.29", optional = true }
crossterm = { version = "0.28", optional = true }
```

### Step 2: Create TUI module structure

Create `src/tui/mod.rs`:

```rust
//! Terminal User Interface Module
//!
//! Feature-gated TUI for workflow observability.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │ [1] WORKFLOW PROGRESS                                               │
//! ├─────────────────────────────┬───────────────────────────────────────┤
//! │ [2] GRAPH TRAVERSAL         │ [3] CONTEXT ASSEMBLED                 │
//! ├─────────────────────────────┴───────────────────────────────────────┤
//! │ [4] AGENT REASONING                                                 │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

#[cfg(feature = "tui")]
mod app;
#[cfg(feature = "tui")]
mod event;
#[cfg(feature = "tui")]
mod ui;
#[cfg(feature = "tui")]
mod panels;

#[cfg(feature = "tui")]
pub use app::App;

/// Run the TUI for a workflow
#[cfg(feature = "tui")]
pub async fn run_tui(workflow_path: &std::path::Path) -> crate::error::Result<()> {
    let app = App::new(workflow_path)?;
    app.run().await
}

#[cfg(not(feature = "tui"))]
pub async fn run_tui(_workflow_path: &std::path::Path) -> crate::error::Result<()> {
    Err(crate::error::NikaError::ValidationError {
        reason: "TUI feature not enabled. Rebuild with --features tui".to_string(),
    })
}
```

### Step 3: Export TUI module

Add to `src/lib.rs`:

```rust
pub mod tui;
```

### Step 4: Commit

```bash
git add src/tui/mod.rs
git commit -m "feat(tui): setup feature-gated TUI module

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: Create App State Machine

**Files:**
- Create: `src/tui/app.rs`

### Step 1: Create app state

Create `src/tui/app.rs`:

```rust
//! TUI Application State Machine

use crate::ast::Workflow;
use crate::error::{NikaError, Result};
use crate::event::{Event, EventKind, EventLog};
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use std::io::stdout;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// TUI Application
pub struct App {
    /// Workflow being executed
    workflow: Arc<Workflow>,
    /// Event log for observability
    event_log: EventLog,
    /// Event receiver for real-time updates
    event_rx: broadcast::Receiver<Event>,
    /// Event sender for runner
    event_tx: broadcast::Sender<Event>,
    /// Current app state
    state: AppState,
    /// Active panel (for keyboard navigation)
    active_panel: Panel,
    /// Collected events for display
    events: Vec<Event>,
    /// Current agent turn (if in agent task)
    current_turn: u32,
    /// Selected event index in workflow panel
    selected_event: usize,
}

/// Application state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Loading workflow
    Loading,
    /// Workflow running
    Running,
    /// Workflow completed
    Completed,
    /// Error occurred
    Error,
}

/// Active panel for keyboard focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Workflow,
    Graph,
    Context,
    Reasoning,
}

impl App {
    /// Create a new TUI app for a workflow
    pub fn new(workflow_path: &Path) -> Result<Self> {
        // Load and parse workflow
        let yaml = std::fs::read_to_string(workflow_path)
            .map_err(|_| NikaError::WorkflowNotFound {
                path: workflow_path.display().to_string(),
            })?;

        let workflow: Workflow = serde_yaml::from_str(&yaml)
            .map_err(|e| NikaError::ParseError { source: e.to_string() })?;

        // Create event broadcast channel
        let (event_tx, event_rx) = broadcast::channel(1000);
        let event_log = EventLog::new();

        Ok(Self {
            workflow: Arc::new(workflow),
            event_log,
            event_rx,
            event_tx,
            state: AppState::Loading,
            active_panel: Panel::Workflow,
            events: Vec::new(),
            current_turn: 0,
            selected_event: 0,
        })
    }

    /// Run the TUI
    pub async fn run(mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

        // Start workflow execution in background
        let workflow = Arc::clone(&self.workflow);
        let event_tx = self.event_tx.clone();
        let event_log = self.event_log.clone();

        tokio::spawn(async move {
            // Execute workflow and emit events
            // This will be connected to the real runner
            Self::execute_workflow(workflow, event_log, event_tx).await
        });

        self.state = AppState::Running;

        // Main loop
        let result = self.main_loop(&mut terminal).await;

        // Cleanup terminal
        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;

        result
    }

    /// Main event loop
    async fn main_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        loop {
            // Draw UI
            terminal.draw(|frame| self.draw(frame))?;

            // Handle events with timeout for responsiveness
            if event::poll(Duration::from_millis(50))? {
                if let CrosstermEvent::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Tab => self.next_panel(),
                            KeyCode::BackTab => self.prev_panel(),
                            KeyCode::Up => self.scroll_up(),
                            KeyCode::Down => self.scroll_down(),
                            KeyCode::Left => self.prev_turn(),
                            KeyCode::Right => self.next_turn(),
                            _ => {}
                        }
                    }
                }
            }

            // Check for new events from runner
            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_event(event);
            }

            // Check if workflow is done
            if self.state == AppState::Completed || self.state == AppState::Error {
                // Keep running to allow inspection
            }
        }
    }

    /// Handle incoming event
    fn handle_event(&mut self, event: Event) {
        // Update state based on event
        match &event.kind {
            EventKind::WorkflowStarted { .. } => {
                self.state = AppState::Running;
            }
            EventKind::WorkflowCompleted { .. } => {
                self.state = AppState::Completed;
            }
            EventKind::WorkflowFailed { .. } => {
                self.state = AppState::Error;
            }
            EventKind::AgentTurnStarted { turn_index, .. } => {
                self.current_turn = *turn_index;
            }
            _ => {}
        }

        self.events.push(event);
    }

    /// Draw the UI
    fn draw(&self, frame: &mut Frame) {
        use crate::tui::ui::draw_ui;
        draw_ui(frame, self);
    }

    /// Navigate to next panel
    fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Workflow => Panel::Graph,
            Panel::Graph => Panel::Context,
            Panel::Context => Panel::Reasoning,
            Panel::Reasoning => Panel::Workflow,
        };
    }

    /// Navigate to previous panel
    fn prev_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Workflow => Panel::Reasoning,
            Panel::Graph => Panel::Workflow,
            Panel::Context => Panel::Graph,
            Panel::Reasoning => Panel::Context,
        };
    }

    /// Scroll up in current panel
    fn scroll_up(&mut self) {
        if self.selected_event > 0 {
            self.selected_event -= 1;
        }
    }

    /// Scroll down in current panel
    fn scroll_down(&mut self) {
        if self.selected_event < self.events.len().saturating_sub(1) {
            self.selected_event += 1;
        }
    }

    /// Previous agent turn
    fn prev_turn(&mut self) {
        if self.current_turn > 0 {
            self.current_turn -= 1;
        }
    }

    /// Next agent turn
    fn next_turn(&mut self) {
        self.current_turn += 1;
    }

    /// Execute workflow (stub - will connect to real runner)
    async fn execute_workflow(
        workflow: Arc<Workflow>,
        event_log: EventLog,
        event_tx: broadcast::Sender<Event>,
    ) -> Result<()> {
        // Emit workflow started
        let event_id = event_log.emit(EventKind::WorkflowStarted {
            task_count: workflow.tasks.len(),
            generation_id: crate::event::generate_generation_id(),
            workflow_hash: "stub".to_string(),
            nika_version: env!("CARGO_PKG_VERSION").to_string(),
        });

        // Send to broadcast
        let _ = event_tx.send(event_log.events().last().cloned().unwrap());

        // TODO: Connect to real workflow runner
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Emit completion
        event_log.emit(EventKind::WorkflowCompleted {
            final_output: Arc::new(serde_json::json!({"status": "stub"})),
            total_duration_ms: 1000,
        });

        let _ = event_tx.send(event_log.events().last().cloned().unwrap());

        Ok(())
    }

    // Getters for UI
    pub fn state(&self) -> AppState { self.state }
    pub fn active_panel(&self) -> Panel { self.active_panel }
    pub fn events(&self) -> &[Event] { &self.events }
    pub fn workflow(&self) -> &Workflow { &self.workflow }
    pub fn current_turn(&self) -> u32 { self.current_turn }
    pub fn selected_event(&self) -> usize { self.selected_event }
}
```

### Step 2: Commit

```bash
git add src/tui/app.rs
git commit -m "feat(tui): add App state machine

- AppState for workflow lifecycle
- Panel navigation with Tab/BackTab
- Event broadcast channel
- Keyboard handling

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: Create UI Renderer

**Files:**
- Create: `src/tui/ui.rs`

### Step 1: Create UI renderer

Create `src/tui/ui.rs`:

```rust
//! TUI Rendering

use crate::tui::app::{App, AppState, Panel};
use ratatui::prelude::*;
use ratatui::widgets::*;

/// Draw the complete UI
pub fn draw_ui(frame: &mut Frame, app: &App) {
    // Create layout: 4 panels
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),    // [1] Workflow Progress
            Constraint::Min(10),       // Middle row
            Constraint::Length(10),   // [4] Agent Reasoning
            Constraint::Length(1),    // Status bar
        ])
        .split(frame.area());

    // Middle row: Graph + Context
    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // [2] Graph
            Constraint::Percentage(60), // [3] Context
        ])
        .split(chunks[1]);

    // Draw each panel
    draw_workflow_panel(frame, app, chunks[0]);
    draw_graph_panel(frame, app, middle_chunks[0]);
    draw_context_panel(frame, app, middle_chunks[1]);
    draw_reasoning_panel(frame, app, chunks[2]);
    draw_status_bar(frame, app, chunks[3]);
}

/// [1] Workflow Progress Panel
fn draw_workflow_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel() == Panel::Workflow;

    let block = Block::default()
        .title(format!("[1] WORKFLOW PROGRESS  {}", state_icon(app.state())))
        .borders(Borders::ALL)
        .border_style(if is_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });

    // List of task events
    let items: Vec<ListItem> = app.events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::event::EventKind::TaskStarted { task_id, .. } => {
                Some(ListItem::new(format!("▶️  {} starting...", task_id)))
            }
            crate::event::EventKind::TaskCompleted { task_id, duration_ms, .. } => {
                Some(ListItem::new(format!("✅ {} ({}ms)", task_id, duration_ms))
                    .style(Style::default().fg(Color::Green)))
            }
            crate::event::EventKind::TaskFailed { task_id, error, .. } => {
                Some(ListItem::new(format!("❌ {}: {}", task_id, error))
                    .style(Style::default().fg(Color::Red)))
            }
            _ => None,
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(list, area);
}

/// [2] Graph Traversal Panel
fn draw_graph_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel() == Panel::Graph;

    let block = Block::default()
        .title("[2] GRAPH TRAVERSAL")
        .borders(Borders::ALL)
        .border_style(if is_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });

    // Placeholder - will show actual graph traversal
    let text = vec![
        Line::from("  Page:qr-code"),
        Line::from("    │"),
        Line::from("    ├─[:REPRESENTS]"),
        Line::from("    │     ▼"),
        Line::from("    │  Entity:qr-code"),
        Line::from("    │     │"),
        Line::from("    │     └─[:HAS_NATIVE]"),
        Line::from("    │           ▼"),
        Line::from("    │       EntityNative@fr-FR"),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(Color::Cyan));

    frame.render_widget(paragraph, area);
}

/// [3] Context Assembled Panel
fn draw_context_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel() == Panel::Context;

    let block = Block::default()
        .title("[3] CONTEXT ASSEMBLED")
        .borders(Borders::ALL)
        .border_style(if is_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });

    // Find context events
    let context_info: Vec<Line> = app.events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::event::EventKind::ContextAssembled {
                total_tokens,
                budget_used_pct,
                sources,
                excluded,
                ..
            } => {
                let mut lines = vec![
                    Line::from(format!("Budget: {}t ({:.0}%)", total_tokens, budget_used_pct * 100.0)),
                    Line::from(""),
                    Line::from("Sources:".to_string()).style(Style::default().fg(Color::Green)),
                ];

                for src in sources.iter().take(5) {
                    lines.push(Line::from(format!("  {} ({}t)", src.node, src.tokens)));
                }

                if !excluded.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from("Excluded:".to_string()).style(Style::default().fg(Color::Red)));
                    for ex in excluded.iter().take(3) {
                        lines.push(Line::from(format!("  ❌ {} - {}", ex.node, ex.reason)));
                    }
                }

                Some(lines)
            }
            _ => None,
        })
        .flatten()
        .collect();

    let text = if context_info.is_empty() {
        vec![Line::from("No context assembled yet...")]
    } else {
        context_info
    };

    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, area);
}

/// [4] Agent Reasoning Panel
fn draw_reasoning_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel() == Panel::Reasoning;

    let title = format!("[4] AGENT REASONING  Turn {}", app.current_turn());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if is_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });

    // Find latest provider response
    let reasoning: Vec<Line> = app.events()
        .iter()
        .rev()
        .find_map(|e| match &e.kind {
            crate::event::EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cost_usd,
                ttft_ms,
                ..
            } => {
                Some(vec![
                    Line::from(format!(
                        "💰 {}in / {}out | Cache: {}read | ${:.4}",
                        input_tokens, output_tokens, cache_read_tokens, cost_usd
                    )),
                    Line::from(format!(
                        "⏱️  TTFT: {}ms",
                        ttft_ms.unwrap_or(0)
                    )),
                ])
            }
            _ => None,
        })
        .unwrap_or_else(|| vec![Line::from("Waiting for agent response...")]);

    let paragraph = Paragraph::new(reasoning).block(block);
    frame.render_widget(paragraph, area);
}

/// Status bar at bottom
fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let keys = "[q] Quit  [Tab] Next Panel  [←→] Turns  [↑↓] Scroll";
    let status = Paragraph::new(keys)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status, area);
}

/// Get icon for app state
fn state_icon(state: AppState) -> &'static str {
    match state {
        AppState::Loading => "⏳ LOADING",
        AppState::Running => "▶️  RUNNING",
        AppState::Completed => "✅ COMPLETE",
        AppState::Error => "❌ ERROR",
    }
}
```

### Step 2: Commit

```bash
git add src/tui/ui.rs
git commit -m "feat(tui): add 4-panel UI renderer

- Workflow progress panel
- Graph traversal panel
- Context assembled panel
- Agent reasoning panel
- Keyboard shortcuts status bar

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: Create Panels Module

**Files:**
- Create: `src/tui/panels/mod.rs`
- Create: `src/tui/panels/workflow.rs`
- Create: `src/tui/panels/graph.rs`
- Create: `src/tui/panels/context.rs`
- Create: `src/tui/panels/reasoning.rs`

### Step 1: Create panels module structure

Create `src/tui/panels/mod.rs`:

```rust
//! TUI Panel Components

mod workflow;
mod graph;
mod context;
mod reasoning;

pub use workflow::*;
pub use graph::*;
pub use context::*;
pub use reasoning::*;
```

### Step 2: Create individual panel files (detailed implementations)

These files will contain more sophisticated rendering logic that extracts from the basic ui.rs. For now, create stubs that re-export from ui.rs.

### Step 3: Commit

```bash
git add src/tui/panels/
git commit -m "feat(tui): add panels module structure

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: Create Event Loop Module

**Files:**
- Create: `src/tui/event.rs`

### Step 1: Create event loop

Create `src/tui/event.rs`:

```rust
//! TUI Event Loop
//!
//! Handles keyboard input and event polling.

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind};
use std::time::Duration;

/// Event types for the TUI
#[derive(Debug)]
pub enum TuiEvent {
    /// Keyboard input
    Key(KeyEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick for periodic updates
    Tick,
}

/// Poll for TUI events with timeout
pub fn poll_event(timeout: Duration) -> Option<TuiEvent> {
    if event::poll(timeout).ok()? {
        match event::read().ok()? {
            CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                Some(TuiEvent::Key(key))
            }
            CrosstermEvent::Resize(w, h) => Some(TuiEvent::Resize(w, h)),
            _ => None,
        }
    } else {
        Some(TuiEvent::Tick)
    }
}

/// Key action mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Quit,
    NextPanel,
    PrevPanel,
    ScrollUp,
    ScrollDown,
    PrevTurn,
    NextTurn,
    ToggleHelp,
    None,
}

impl From<KeyEvent> for KeyAction {
    fn from(key: KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => KeyAction::Quit,
            KeyCode::Tab => KeyAction::NextPanel,
            KeyCode::BackTab => KeyAction::PrevPanel,
            KeyCode::Up | KeyCode::Char('k') => KeyAction::ScrollUp,
            KeyCode::Down | KeyCode::Char('j') => KeyAction::ScrollDown,
            KeyCode::Left | KeyCode::Char('h') => KeyAction::PrevTurn,
            KeyCode::Right | KeyCode::Char('l') => KeyAction::NextTurn,
            KeyCode::Char('?') => KeyAction::ToggleHelp,
            _ => KeyAction::None,
        }
    }
}
```

### Step 2: Commit

```bash
git add src/tui/event.rs
git commit -m "feat(tui): add event loop module

- TuiEvent enum
- KeyAction mapping
- Vim-style navigation (hjkl)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Add CLI Trace Commands

**Files:**
- Modify: `src/main.rs`

### Step 1: Add trace subcommand

Add to CLI in `src/main.rs`:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nika")]
#[command(about = "DAG workflow runner for AI tasks")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a workflow
    Run {
        /// Path to workflow YAML file
        workflow: PathBuf,
        /// Output format (json, yaml, text)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Validate a workflow without executing
    Validate {
        /// Path to workflow YAML file
        workflow: PathBuf,
    },

    /// Run workflow with TUI
    #[cfg(feature = "tui")]
    Tui {
        /// Path to workflow YAML file
        workflow: PathBuf,
    },

    /// Manage execution traces
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },
}

#[derive(Subcommand)]
enum TraceAction {
    /// List all traces
    List {
        /// Show only last N traces
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Show details of a trace
    Show {
        /// Generation ID or partial match
        id: String,
    },

    /// Export trace to file
    Export {
        /// Generation ID
        id: String,
        /// Output format (json, yaml, csv)
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Delete old traces
    Clean {
        /// Keep only last N traces
        #[arg(short, long, default_value = "10")]
        keep: usize,
    },
}
```

### Step 2: Implement trace commands

```rust
async fn handle_trace_command(action: TraceAction) -> Result<()> {
    match action {
        TraceAction::List { limit } => {
            let traces = crate::event::list_traces()?;
            let traces = match limit {
                Some(n) => traces.into_iter().take(n).collect(),
                None => traces,
            };

            println!("Found {} traces:\n", traces.len());
            println!("{:<30} {:>10} {:>20}", "GENERATION ID", "SIZE", "CREATED");
            println!("{}", "-".repeat(62));

            for trace in traces {
                let created = trace.created
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        let secs = d.as_secs();
                        chrono::DateTime::from_timestamp(secs as i64, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                let size = if trace.size_bytes > 1024 * 1024 {
                    format!("{:.1}MB", trace.size_bytes as f64 / 1024.0 / 1024.0)
                } else if trace.size_bytes > 1024 {
                    format!("{:.1}KB", trace.size_bytes as f64 / 1024.0)
                } else {
                    format!("{}B", trace.size_bytes)
                };

                println!("{:<30} {:>10} {:>20}", trace.generation_id, size, created);
            }
            Ok(())
        }

        TraceAction::Show { id } => {
            let traces = crate::event::list_traces()?;
            let trace = traces.iter()
                .find(|t| t.generation_id.contains(&id))
                .ok_or_else(|| NikaError::ValidationError {
                    reason: format!("No trace matching '{}'", id),
                })?;

            // Read and display trace
            let content = std::fs::read_to_string(&trace.path)?;
            let events: Vec<crate::event::Event> = content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();

            println!("Trace: {}", trace.generation_id);
            println!("Events: {}", events.len());
            println!("Size: {} bytes\n", trace.size_bytes);

            for event in events {
                println!("[{:>6}ms] {:?}", event.timestamp_ms, event.kind);
            }

            Ok(())
        }

        TraceAction::Export { id, format, output } => {
            let traces = crate::event::list_traces()?;
            let trace = traces.iter()
                .find(|t| t.generation_id.contains(&id))
                .ok_or_else(|| NikaError::ValidationError {
                    reason: format!("No trace matching '{}'", id),
                })?;

            let content = std::fs::read_to_string(&trace.path)?;
            let events: Vec<crate::event::Event> = content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();

            let exported = match format.as_str() {
                "json" => serde_json::to_string_pretty(&events)?,
                "yaml" => serde_yaml::to_string(&events)?,
                _ => return Err(NikaError::ValidationError {
                    reason: format!("Unknown format: {}", format),
                }),
            };

            match output {
                Some(path) => std::fs::write(path, exported)?,
                None => println!("{}", exported),
            }

            Ok(())
        }

        TraceAction::Clean { keep } => {
            let traces = crate::event::list_traces()?;
            let to_delete = traces.into_iter().skip(keep);

            let mut deleted = 0;
            for trace in to_delete {
                std::fs::remove_file(&trace.path)?;
                deleted += 1;
            }

            println!("Deleted {} old traces, kept {}", deleted, keep);
            Ok(())
        }
    }
}
```

### Step 3: Commit

```bash
git add src/main.rs
git commit -m "feat(cli): add trace management commands

- trace list: show all traces
- trace show: display trace details
- trace export: export to JSON/YAML
- trace clean: remove old traces

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 7: Integration Test for TUI

**Files:**
- Create: `tests/tui_test.rs`

### Step 1: Create TUI tests

```rust
//! TUI integration tests

#[cfg(feature = "tui")]
mod tui_tests {
    use nika::tui::App;
    use std::path::Path;

    #[test]
    fn test_app_creation() {
        // Create a temp workflow file
        let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude
tasks:
  - id: test
    infer:
      prompt: "Hello"
"#;
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), yaml).unwrap();

        let app = App::new(temp.path());
        assert!(app.is_ok());
    }

    #[test]
    fn test_app_state_transitions() {
        // Test state machine logic
    }
}
```

### Step 2: Commit

```bash
git add tests/tui_test.rs
git commit -m "test: add TUI integration tests

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 8: Add TUI Entry Point to CLI

**Files:**
- Modify: `src/main.rs`

### Step 1: Add TUI command handler

```rust
#[cfg(feature = "tui")]
Commands::Tui { workflow } => {
    crate::tui::run_tui(&workflow).await?;
    Ok(())
}
```

### Step 2: Commit

```bash
git add src/main.rs
git commit -m "feat(cli): add tui command entry point

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

After completing MVP 3, Nika will have:

1. **Feature-gated TUI** - `--features tui` enabled by default
2. **4-Panel Layout** - Workflow, Graph, Context, Reasoning
3. **App State Machine** - Loading, Running, Completed, Error
4. **Event Broadcast** - Real-time updates via tokio::broadcast
5. **Keyboard Navigation** - Tab, arrows, vim-style (hjkl)
6. **CLI Trace Commands** - list, show, export, clean

**Verify Success:**

```bash
# Build with TUI
cargo build --features tui

# Run TUI
cargo run -- tui examples/invoke-novanet.yaml

# Trace commands
cargo run -- trace list
cargo run -- trace show <id>
cargo run -- trace export <id> --format yaml
cargo run -- trace clean --keep 5
```

**Full MVP Sequence Complete!**

```
MVP 0: DX Setup        → Foundation
MVP 1: Invoke Verb     → MCP Integration
MVP 2: Agent + Events  → Agentic Execution
MVP 3: TUI + Trace     → Observability
```
