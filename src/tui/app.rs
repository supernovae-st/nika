//! TUI Application - Main entry point and run loop

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

use super::events::{handle_key_event, poll_event, Action};
use super::runtime::{MockRuntime, RuntimeBridge};
use super::state::{ActivityEvent, AppState, WorkflowStatus};
use super::theme::{icons, HyperspaceTheme};

/// TUI Application
pub struct TuiApp {
    state: AppState,
    theme: HyperspaceTheme,
    runtime: Box<dyn RuntimeBridge>,
}

impl TuiApp {
    /// Create a new TUI application
    pub fn new(_workflow_path: Option<&str>) -> anyhow::Result<Self> {
        let state = AppState::default();
        let theme = HyperspaceTheme::new();
        let runtime: Box<dyn RuntimeBridge> = Box::new(MockRuntime::new());

        Ok(Self {
            state,
            theme,
            runtime,
        })
    }

    /// Run the TUI application
    pub async fn run(mut self) -> anyhow::Result<()> {
        // Setup terminal
        let mut terminal = self.setup_terminal()?;

        // Load demo workflow
        self.state.workflow_name = "demo-workflow".to_string();
        self.state
            .push_event(ActivityEvent::info("TUI Dashboard started"));
        self.state
            .push_event(ActivityEvent::info("Press 'q' to quit, Tab to navigate"));

        // Start the mock runtime
        self.runtime.start().await?;
        self.state.status = WorkflowStatus::Running;
        self.state.start_time = Some(std::time::Instant::now());

        // Main loop
        let result = self.main_loop(&mut terminal).await;

        // Restore terminal
        self.restore_terminal(&mut terminal)?;

        result
    }

    /// Setup terminal for TUI
    fn setup_terminal(&self) -> anyhow::Result<Terminal<CrosstermBackend<Stdout>>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(terminal)
    }

    /// Restore terminal to normal state
    fn restore_terminal(
        &self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> anyhow::Result<()> {
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        Ok(())
    }

    /// Main event loop
    async fn main_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> anyhow::Result<()> {
        let tick_rate = Duration::from_millis(16); // ~60fps

        loop {
            // Update elapsed time
            self.state.tick();

            // Render
            terminal.draw(|frame| self.render(frame))?;

            // Poll for events
            if let Some(key) = poll_event(tick_rate)? {
                let action = handle_key_event(key, &mut self.state);
                if action == Action::Quit {
                    self.state.should_quit = true;
                    break;
                }
            }

            // Check if should quit
            if self.state.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// Render the UI
    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        // Main layout: Header, Content, Footer
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Content
                Constraint::Length(3), // Context bar
                Constraint::Length(1), // Footer
            ])
            .split(area);

        // Render header
        self.render_header(frame, main_chunks[0]);

        // Content layout: Left (DAG + Activity) | Right (Stats + Agents + Connections)
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(main_chunks[1]);

        // Left side: DAG on top, Activity below
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(content_chunks[0]);

        self.render_dag(frame, left_chunks[0]);
        self.render_activity(frame, left_chunks[1]);

        // Right side: Session, Subagents, Connections
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7), // Session stats
                Constraint::Min(5),    // Subagents
                Constraint::Length(6), // Connections
            ])
            .split(content_chunks[1]);

        self.render_session(frame, right_chunks[0]);
        self.render_subagents(frame, right_chunks[1]);
        self.render_connections(frame, right_chunks[2]);

        // Context temperature bar
        self.render_context_bar(frame, main_chunks[2]);

        // Footer
        self.render_footer(frame, main_chunks[3]);
    }

    /// Render header
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let elapsed = format!(
            "{:02}:{:02}:{:02}",
            self.state.elapsed.as_secs() / 3600,
            (self.state.elapsed.as_secs() % 3600) / 60,
            self.state.elapsed.as_secs() % 60
        );

        let status_style = match self.state.status {
            WorkflowStatus::Running => self.theme.success(),
            WorkflowStatus::Paused => self.theme.warning(),
            WorkflowStatus::Failed => self.theme.error(),
            WorkflowStatus::Completed => self.theme.highlight(),
            _ => self.theme.dimmed(),
        };

        let header = Line::from(vec![
            Span::styled(
                format!("{} NIKA v0.1.0", icons::MAIN_AGENT),
                self.theme.header(),
            ),
            Span::raw("  │  "),
            Span::styled(&self.state.workflow_name, self.theme.accent()),
            Span::raw("  │  "),
            Span::styled(format!("{}", self.state.status), status_style),
            Span::raw("  │  "),
            Span::styled(format!("⏱ {}", elapsed), self.theme.text()),
            Span::raw("  │  "),
            Span::styled("F1:Help", self.theme.dimmed()),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.context_border())
            .title(" MISSION CONTROL ");

        let paragraph = Paragraph::new(header).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render DAG panel
    fn render_dag(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.state.focus == super::state::PanelFocus::Dag;
        let border_style = if is_focused {
            self.theme.highlight()
        } else {
            self.theme.dimmed()
        };

        // Simple DAG visualization (placeholder for now)
        let dag_text = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("    "),
                Span::styled("[analyze]", Style::default().fg(self.theme.space_violet)),
                Span::raw(" ──► "),
                Span::styled("[generate]", Style::default().fg(self.theme.space_violet)),
            ]),
            Line::from(vec![Span::raw("        │              │")]),
            Line::from(vec![Span::raw("        ▼              ▼")]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled("[transform]", Style::default().fg(self.theme.amber_gold)),
                Span::raw(" ──► "),
                Span::styled("[review]", Style::default().fg(self.theme.cyan_teal)),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!(" {} WORKFLOW DAG ", icons::PORTAL));

        let paragraph = Paragraph::new(dag_text).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render session stats panel
    fn render_session(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.state.focus == super::state::PanelFocus::Session;
        let border_style = if is_focused {
            self.theme.highlight()
        } else {
            self.theme.dimmed()
        };

        let completed = self.state.completed_tasks();
        let total = self.state.total_tasks().max(4); // Demo shows 4
        let progress = self.state.task_progress();
        let progress_bar = self.make_progress_bar(progress, 12);

        let stats = vec![
            Line::from(vec![
                Span::raw("  Tasks:  "),
                Span::styled(format!("{}/{}", completed, total), self.theme.text()),
                Span::raw(" "),
                Span::styled(progress_bar, self.theme.accent()),
            ]),
            Line::from(vec![
                Span::raw("  Tokens: "),
                Span::styled(
                    format!("{}K/200K", self.state.tokens.total / 1000),
                    self.theme.text(),
                ),
            ]),
            Line::from(vec![
                Span::raw("  Cost:   "),
                Span::styled(format!("${:.4}", self.state.tokens.cost), self.theme.text()),
            ]),
            Line::from(vec![
                Span::raw("  Errors: "),
                Span::styled("0", self.theme.success()),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" 📊 SESSION ");

        let paragraph = Paragraph::new(stats).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render subagents panel
    fn render_subagents(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.state.focus == super::state::PanelFocus::Subagents;
        let border_style = if is_focused {
            self.theme.highlight()
        } else {
            self.theme.dimmed()
        };

        // Demo agents
        let agents = vec![
            Line::from(vec![
                Span::raw("  "),
                Span::styled(icons::MAIN_AGENT, self.theme.success()),
                Span::raw(" analyzer  "),
                Span::styled(
                    self.make_progress_bar(80.0, 10),
                    Style::default().fg(self.theme.space_violet),
                ),
                Span::raw(" "),
                Span::styled(icons::CONTEXT, self.theme.text()),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(icons::SUBAGENT, self.theme.dimmed()),
                Span::raw(" generator "),
                Span::styled(
                    self.make_progress_bar(45.0, 10),
                    Style::default().fg(self.theme.space_violet),
                ),
                Span::raw(" "),
                Span::styled(icons::CONTEXT, self.theme.text()),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(icons::SUBAGENT, self.theme.dimmed()),
                Span::raw(" reviewer  "),
                Span::styled(
                    self.make_progress_bar(0.0, 10),
                    Style::default().fg(self.theme.cyan_teal),
                ),
                Span::raw(" "),
                Span::styled(icons::ISOLATED, self.theme.text()),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!(" {} SUBAGENTS ", icons::ISOLATED));

        let paragraph = Paragraph::new(agents).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render activity log panel
    fn render_activity(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.state.focus == super::state::PanelFocus::Activity;
        let border_style = if is_focused {
            self.theme.highlight()
        } else {
            self.theme.dimmed()
        };

        let events: Vec<Line> = self
            .state
            .events
            .iter()
            .take(area.height.saturating_sub(2) as usize)
            .map(|e| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(e.icon(), self.theme.text()),
                    Span::raw(" "),
                    Span::styled(&e.message, self.theme.text()),
                ])
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" 📋 ACTIVITY ");

        let paragraph = Paragraph::new(events).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render connections panel
    fn render_connections(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.state.focus == super::state::PanelFocus::Connections;
        let border_style = if is_focused {
            self.theme.highlight()
        } else {
            self.theme.dimmed()
        };

        let connections = vec![
            Line::from(vec![
                Span::raw("  "),
                Span::styled(icons::MCP, self.theme.success()),
                Span::raw(" filesystem "),
                Span::styled("[CONNECTED]", self.theme.success()),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(icons::MCP, self.theme.success()),
                Span::raw(" github     "),
                Span::styled("[CONNECTED]", self.theme.success()),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(icons::SKILL, self.theme.accent()),
                Span::raw(" code-review "),
                Span::styled("[LOADED]", self.theme.accent()),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!(" {} CONNECTIONS ", icons::MCP));

        let paragraph = Paragraph::new(connections).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render context temperature bar
    fn render_context_bar(&self, frame: &mut Frame, area: Rect) {
        let usage = self.state.context_usage().max(35.0); // Demo value
        let bar_width = (area.width.saturating_sub(30)) as usize;
        let filled = ((usage / 100.0) * bar_width as f32) as usize;
        let empty = bar_width.saturating_sub(filled);

        let color = self.theme.temperature_color(usage);

        let bar = format!(
            "  📈 CONTEXT TEMPERATURE  [{}{}] {:.0}% ({}K)",
            icons::BAR_FULL.to_string().repeat(filled),
            icons::BAR_EMPTY.to_string().repeat(empty),
            usage,
            (usage * 2.0) as u32 // Fake token count
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color));

        let paragraph =
            Paragraph::new(Line::from(Span::styled(bar, Style::default().fg(color)))).block(block);
        frame.render_widget(paragraph, area);
    }

    /// Render footer
    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help = Line::from(vec![
            Span::styled(" [q]", self.theme.accent()),
            Span::styled("uit  ", self.theme.dimmed()),
            Span::styled("[p]", self.theme.accent()),
            Span::styled("ause  ", self.theme.dimmed()),
            Span::styled("[r]", self.theme.accent()),
            Span::styled("estart  ", self.theme.dimmed()),
            Span::styled("[Tab]", self.theme.accent()),
            Span::styled(" focus  ", self.theme.dimmed()),
            Span::styled("[↑↓]", self.theme.accent()),
            Span::styled(" scroll  ", self.theme.dimmed()),
            Span::styled("[Enter]", self.theme.accent()),
            Span::styled(" select", self.theme.dimmed()),
        ]);

        let paragraph = Paragraph::new(help);
        frame.render_widget(paragraph, area);
    }

    /// Create a progress bar string
    fn make_progress_bar(&self, percent: f32, width: usize) -> String {
        let filled = ((percent / 100.0) * width as f32) as usize;
        let empty = width.saturating_sub(filled);
        format!(
            "[{}{}]",
            icons::BAR_FULL.to_string().repeat(filled),
            icons::BAR_EMPTY.to_string().repeat(empty)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar() {
        let app = TuiApp::new(None).unwrap();
        let bar = app.make_progress_bar(50.0, 10);
        assert!(bar.contains("█████"));
        assert!(bar.contains("░░░░░"));
    }
}
