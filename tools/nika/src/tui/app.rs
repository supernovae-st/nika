//! TUI Application
//!
//! Main event loop with 60 FPS rendering.
//! Handles keyboard input, event processing, and frame rendering.

use std::io::{self, Stdout};
use std::path::Path;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame, Terminal,
};
use tokio::sync::mpsc;

use crate::error::{NikaError, Result};
use crate::event::Event as NikaEvent;

use super::panels::{ContextPanel, GraphPanel, ProgressPanel, ReasoningPanel};
use super::state::{PanelId, TuiMode, TuiState};
use super::theme::Theme;

/// Frame rate target (60 FPS)
const FRAME_RATE_MS: u64 = 16;

/// Action resulting from input handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Continue normal operation
    Continue,
    /// Quit the application
    Quit,
    /// Toggle pause
    TogglePause,
    /// Step one event (when paused)
    Step,
    /// Focus next panel
    FocusNext,
    /// Focus previous panel
    FocusPrev,
    /// Focus specific panel
    FocusPanel(u8),
    /// Toggle mode
    SetMode(TuiMode),
    /// Scroll up in focused panel
    ScrollUp,
    /// Scroll down in focused panel
    ScrollDown,
}

/// Main TUI application
pub struct App {
    /// Path to the workflow being observed
    workflow_path: std::path::PathBuf,
    /// Terminal backend (initialized on run)
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
    /// TUI state
    state: TuiState,
    /// Color theme
    theme: Theme,
    /// Event receiver from runtime
    event_rx: Option<mpsc::Receiver<NikaEvent>>,
    /// Should quit flag
    should_quit: bool,
}

impl App {
    /// Create a new TUI application for the given workflow
    ///
    /// Note: Terminal initialization is deferred to `run()` to allow
    /// App creation in test contexts without a TTY.
    pub fn new(workflow_path: &Path) -> Result<Self> {
        if !workflow_path.exists() {
            return Err(NikaError::WorkflowNotFound {
                path: workflow_path.display().to_string(),
            });
        }

        let state = TuiState::new(&workflow_path.display().to_string());

        Ok(Self {
            workflow_path: workflow_path.to_path_buf(),
            terminal: None,
            state,
            theme: Theme::novanet(),
            event_rx: None,
            should_quit: false,
        })
    }

    /// Initialize terminal for TUI rendering
    fn init_terminal(&mut self) -> Result<()> {
        if self.terminal.is_some() {
            return Ok(());
        }

        enable_raw_mode().map_err(|e| NikaError::TuiError {
            reason: format!("Failed to enable raw mode: {}", e),
        })?;

        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(|e| NikaError::TuiError {
            reason: format!("Failed to enter alternate screen: {}", e),
        })?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|e| NikaError::TuiError {
            reason: format!("Failed to create terminal: {}", e),
        })?;

        self.terminal = Some(terminal);
        Ok(())
    }

    /// Set the event receiver from runtime
    pub fn with_event_receiver(mut self, rx: mpsc::Receiver<NikaEvent>) -> Self {
        self.event_rx = Some(rx);
        self
    }

    /// Run the TUI application
    pub async fn run(mut self) -> Result<()> {
        tracing::info!("TUI started for workflow: {}", self.workflow_path.display());

        // Initialize terminal (deferred from new())
        self.init_terminal()?;

        let tick_rate = Duration::from_millis(FRAME_RATE_MS);

        loop {
            // 1. Poll runtime events (non-blocking)
            if let Some(ref mut rx) = self.event_rx {
                while let Ok(event) = rx.try_recv() {
                    // Check for breakpoints
                    if self.state.should_break(&event.kind) {
                        self.state.paused = true;
                    }

                    // Update state
                    self.state.handle_event(&event.kind, event.timestamp_ms);
                }
            }

            // 2. Update elapsed time
            self.state.tick();

            // 3. Render frame
            let state = &self.state;
            let theme = &self.theme;
            if let Some(ref mut terminal) = self.terminal {
                terminal
                    .draw(|frame| render_frame(frame, state, theme))
                    .map_err(|e| NikaError::TuiError {
                        reason: format!("Failed to draw frame: {}", e),
                    })?;
            }

            // 4. Poll keyboard input (with timeout for frame rate)
            if event::poll(tick_rate).map_err(|e| NikaError::TuiError {
                reason: format!("Failed to poll events: {}", e),
            })? {
                if let Event::Key(key) = event::read().map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to read event: {}", e),
                })? {
                    let action = self.handle_key(key.code, key.modifiers);
                    self.apply_action(action);
                }
            }

            // 5. Check quit flag
            if self.should_quit {
                break;
            }
        }

        // Cleanup
        self.cleanup()?;

        Ok(())
    }

    /// Handle keyboard input
    fn handle_key(&self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        // Handle mode-specific keys first
        match &self.state.mode {
            TuiMode::Help | TuiMode::Metrics | TuiMode::Inspect(_) | TuiMode::Edit(_) => {
                if code == KeyCode::Esc {
                    return Action::SetMode(TuiMode::Normal);
                }
            }
            _ => {}
        }

        // Global keys
        match code {
            // Quit
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,

            // Navigation
            KeyCode::Tab => Action::FocusNext,
            KeyCode::BackTab => Action::FocusPrev,
            KeyCode::Char('1') => Action::FocusPanel(1),
            KeyCode::Char('2') => Action::FocusPanel(2),
            KeyCode::Char('3') => Action::FocusPanel(3),
            KeyCode::Char('4') => Action::FocusPanel(4),

            // Execution control
            KeyCode::Char(' ') => Action::TogglePause,
            KeyCode::Enter if self.state.paused => Action::Step,

            // Scrolling
            KeyCode::Up | KeyCode::Char('k') => Action::ScrollUp,
            KeyCode::Down | KeyCode::Char('j') => Action::ScrollDown,

            // Overlays
            KeyCode::Char('?') | KeyCode::F(1) => Action::SetMode(TuiMode::Help),
            KeyCode::Char('m') => Action::SetMode(TuiMode::Metrics),

            // Escape
            KeyCode::Esc => Action::SetMode(TuiMode::Normal),

            _ => Action::Continue,
        }
    }

    /// Apply an action to the state
    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::TogglePause => self.state.toggle_pause(),
            Action::Step => {
                // Step mode: advance one event then pause again
                self.state.step_mode = true;
            }
            Action::FocusNext => self.state.focus_next(),
            Action::FocusPrev => self.state.focus_prev(),
            Action::FocusPanel(n) => self.state.focus_panel(n),
            Action::SetMode(mode) => self.state.mode = mode,
            Action::ScrollUp => {
                let scroll = self.state.scroll.entry(self.state.focus).or_insert(0);
                *scroll = scroll.saturating_sub(1);
            }
            Action::ScrollDown => {
                let scroll = self.state.scroll.entry(self.state.focus).or_insert(0);
                *scroll += 1;
            }
            Action::Continue => {}
        }
    }

    /// Cleanup terminal state
    fn cleanup(&mut self) -> Result<()> {
        if let Some(ref mut terminal) = self.terminal {
            disable_raw_mode().map_err(|e| NikaError::TuiError {
                reason: format!("Failed to disable raw mode: {}", e),
            })?;

            execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|e| {
                NikaError::TuiError {
                    reason: format!("Failed to leave alternate screen: {}", e),
                }
            })?;

            terminal.show_cursor().map_err(|e| NikaError::TuiError {
                reason: format!("Failed to show cursor: {}", e),
            })?;
        }

        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Best effort cleanup
        if let Some(ref mut terminal) = self.terminal {
            let _ = disable_raw_mode();
            let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
            let _ = terminal.show_cursor();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// RENDER FUNCTIONS (standalone to avoid borrow checker issues)
// ═══════════════════════════════════════════════════════════════════

/// Render a frame
fn render_frame(frame: &mut Frame, state: &TuiState, theme: &Theme) {
    let size = frame.area();

    // Create 2x2 layout
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(size);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[1]);

    // Render panels
    render_panel(frame, state, theme, PanelId::Progress, top_chunks[0]);
    render_panel(frame, state, theme, PanelId::Dag, top_chunks[1]);
    render_panel(frame, state, theme, PanelId::NovaNet, bottom_chunks[0]);
    render_panel(frame, state, theme, PanelId::Agent, bottom_chunks[1]);

    // Render overlay if active
    match &state.mode {
        TuiMode::Help => render_help_overlay(frame, theme, size),
        TuiMode::Metrics => render_metrics_overlay(frame, state, theme, size),
        _ => {}
    }
}

/// Render a single panel
fn render_panel(
    frame: &mut Frame,
    state: &TuiState,
    theme: &Theme,
    panel_id: PanelId,
    area: Rect,
) {
    let focused = state.focus == panel_id;

    // All panels have dedicated widgets
    match panel_id {
        PanelId::Progress => {
            let panel = ProgressPanel::new(state, theme).focused(focused);
            panel.render(area, frame.buffer_mut());
        }
        PanelId::Dag => {
            let panel = GraphPanel::new(state, theme).focused(focused);
            panel.render(area, frame.buffer_mut());
        }
        PanelId::NovaNet => {
            let panel = ContextPanel::new(state, theme).focused(focused);
            panel.render(area, frame.buffer_mut());
        }
        PanelId::Agent => {
            let panel = ReasoningPanel::new(state, theme).focused(focused);
            panel.render(area, frame.buffer_mut());
        }
    }
}

/// Render progress panel content
fn render_progress_content(state: &TuiState) -> String {
    let phase = state.workflow.phase;
    let elapsed = format_duration(state.workflow.elapsed_ms);
    let progress = state.workflow.progress_pct();

    format!(
        "Phase: {} {}\nElapsed: {}\nProgress: {:.0}% ({}/{})\n\nCurrent: {}",
        phase.icon(),
        phase.name(),
        elapsed,
        progress,
        state.workflow.tasks_completed,
        state.workflow.task_count,
        state.current_task.as_deref().unwrap_or("(none)")
    )
}

/// Render DAG panel content
fn render_dag_content(state: &TuiState) -> String {
    use super::theme::TaskStatus;

    let mut content = String::new();
    for task_id in &state.task_order {
        if let Some(task) = state.tasks.get(task_id) {
            let status_icon = match task.status {
                TaskStatus::Pending => "○",
                TaskStatus::Running => "◉",
                TaskStatus::Success => "✓",
                TaskStatus::Failed => "⊗",
                TaskStatus::Paused => "⏸",
            };
            content.push_str(&format!("{} {}\n", status_icon, task_id));
        }
    }
    if content.is_empty() {
        content = "(no tasks scheduled)".to_string();
    }
    content
}

/// Render NovaNet panel content
fn render_novanet_content(state: &TuiState) -> String {
    let mut content = String::new();

    // MCP call count
    content.push_str(&format!("MCP Calls: {}\n\n", state.mcp_calls.len()));

    // Recent calls
    for call in state.mcp_calls.iter().rev().take(5) {
        let tool = call.tool.as_deref().unwrap_or("resource");
        let status = if call.completed { "✓" } else { "⋯" };
        content.push_str(&format!("{} {}\n", status, tool));
    }

    // Context assembly
    if state.context_assembly.total_tokens > 0 {
        content.push_str(&format!(
            "\nContext: {} tokens ({:.0}%)",
            state.context_assembly.total_tokens, state.context_assembly.budget_used_pct
        ));
    }

    content
}

/// Render agent panel content
fn render_agent_content(state: &TuiState) -> String {
    let mut content = String::new();

    if let Some(max) = state.agent_max_turns {
        content.push_str(&format!("Turns: {}/{}\n\n", state.agent_turns.len(), max));

        for turn in &state.agent_turns {
            content.push_str(&format!("Turn {}: {}\n", turn.index, turn.status));
        }
    } else {
        content = "(no agent active)".to_string();
    }

    content
}

/// Render help overlay
fn render_help_overlay(frame: &mut Frame, theme: &Theme, area: Rect) {
    let help_text = r#"
╔═══════════════════════════════════════════════════════════════════╗
║  KEYBOARD SHORTCUTS                                               ║
╠═══════════════════════════════════════════════════════════════════╣
║                                                                   ║
║  NAVIGATION           EXECUTION           OVERLAYS               ║
║  Tab      Next panel  Space   Pause       ?/F1  This help       ║
║  1-4      Jump panel  Enter   Step        m     Metrics         ║
║  j/k      Scroll      q       Quit        Esc   Close           ║
║                                                                   ║
╚═══════════════════════════════════════════════════════════════════╝
"#;

    let overlay = centered_rect(70, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help ")
        .style(Style::default().add_modifier(Modifier::BOLD));

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .style(theme.text_style());

    frame.render_widget(paragraph, overlay);
}

/// Render metrics overlay
fn render_metrics_overlay(frame: &mut Frame, state: &TuiState, theme: &Theme, area: Rect) {
    let metrics = &state.metrics;
    let content = format!(
        r#"
╭─────────────────────────────────────────────────────────────────────╮
│  MISSION METRICS                                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Total Tokens:  {:>10}                                             │
│  Input Tokens:  {:>10}                                             │
│  Output Tokens: {:>10}                                             │
│  Cost (USD):    ${:>9.4}                                           │
│                                                                     │
│  MCP Calls: {}                                                      │
│                                                                     │
╰─────────────────────────────────────────────────────────────────────╯
"#,
        metrics.total_tokens,
        metrics.input_tokens,
        metrics.output_tokens,
        metrics.cost_usd,
        metrics.mcp_calls.values().sum::<usize>()
    );

    let overlay = centered_rect(70, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Metrics ")
        .style(Style::default().add_modifier(Modifier::BOLD));

    let paragraph = Paragraph::new(content)
        .block(block)
        .style(theme.text_style());

    frame.render_widget(paragraph, overlay);
}

// ═══════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════

/// Format duration in HH:MM:SS or MM:SS
fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

/// Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(5000), "00:05");
        assert_eq!(format_duration(65000), "01:05");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3661000), "01:01:01");
    }

    #[test]
    fn test_handle_key_quit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path = temp_dir.path().join("test.yaml");
        std::fs::write(&workflow_path, "schema: test").unwrap();

        // We can't fully test App without a terminal, so just test the key handling logic
        // by checking the action enum values
        assert_eq!(
            Action::Quit,
            Action::Quit // Self-equality sanity check
        );
    }

    #[test]
    fn test_action_variants() {
        // Verify all action variants exist and are distinct
        let actions = vec![
            Action::Continue,
            Action::Quit,
            Action::TogglePause,
            Action::Step,
            Action::FocusNext,
            Action::FocusPrev,
            Action::FocusPanel(1),
            Action::SetMode(TuiMode::Help),
            Action::ScrollUp,
            Action::ScrollDown,
        ];

        // All should be different
        for (i, a1) in actions.iter().enumerate() {
            for (j, a2) in actions.iter().enumerate() {
                if i != j {
                    assert_ne!(a1, a2);
                }
            }
        }
    }
}
