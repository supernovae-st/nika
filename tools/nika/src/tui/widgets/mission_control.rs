//! Mission Control Panel - Right sidebar with MCP, Context, Memory, Runtime info
//!
//! Layout:
//! ```text
//! ┌─ 📊 MISSION CONTROL ─────────────────┐
//! │ 🔌 MCP SERVERS                       │
//! │ ● novanet (8 tools)                  │
//! │ ● perplexity (3 tools)               │
//! │ ○ offline-server                     │
//! ├──────────────────────────────────────┤
//! │ 📁 CONTEXT                           │
//! │ ✓ @entity:qr-code (1.2k)            │
//! │ ◐ @fr-FR (loading)                   │
//! │ ○ @landing-page.md (pending)         │
//! ├──────────────────────────────────────┤
//! │ 💾 MEMORY                            │
//! │ • CLAUDE.md (project)                │
//! │ • 3 conversation turns               │
//! ├──────────────────────────────────────┤
//! │ ⚡ RUNTIME                            │
//! │ Current: infer                       │
//! │ In: 2,341 │ Out: 1,203               │
//! │ Cost: $0.08                          │
//! └──────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use super::{McpServerInfo, McpStatus};

// Solarized-inspired colors
const COLOR_HEADER: Color = Color::Rgb(250, 204, 21);     // Gold
const COLOR_SECTION: Color = Color::Rgb(147, 161, 161);   // Gray
const COLOR_SUCCESS: Color = Color::Rgb(133, 153, 0);     // Green
const COLOR_WARNING: Color = Color::Rgb(181, 137, 0);     // Yellow
const COLOR_ERROR: Color = Color::Rgb(220, 50, 47);       // Red
const COLOR_MUTED: Color = Color::Rgb(88, 110, 117);      // Muted
const COLOR_CYAN: Color = Color::Rgb(42, 161, 152);       // Cyan
const COLOR_VIOLET: Color = Color::Rgb(108, 113, 196);    // Violet

/// Verb type for runtime display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CurrentVerb {
    #[default]
    None,
    Infer,
    Exec,
    Fetch,
    Invoke,
    Agent,
}

impl CurrentVerb {
    pub fn icon(&self) -> &'static str {
        match self {
            CurrentVerb::None => "○",
            CurrentVerb::Infer => "⚡",
            CurrentVerb::Exec => "📟",
            CurrentVerb::Fetch => "🛰️",
            CurrentVerb::Invoke => "🔌",
            CurrentVerb::Agent => "🐔",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CurrentVerb::None => "idle",
            CurrentVerb::Infer => "infer",
            CurrentVerb::Exec => "exec",
            CurrentVerb::Fetch => "fetch",
            CurrentVerb::Invoke => "invoke",
            CurrentVerb::Agent => "agent",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            CurrentVerb::None => COLOR_MUTED,
            CurrentVerb::Infer => COLOR_VIOLET,
            CurrentVerb::Exec => COLOR_WARNING,
            CurrentVerb::Fetch => COLOR_CYAN,
            CurrentVerb::Invoke => COLOR_SUCCESS,
            CurrentVerb::Agent => Color::Rgb(236, 72, 153), // Pink
        }
    }
}

/// Context item with loading status
#[derive(Debug, Clone)]
pub struct ContextItem {
    /// The mention string (e.g., "@entity:qr-code")
    pub mention: String,
    /// Loading status
    pub status: ContextStatus,
    /// Token count if loaded
    pub tokens: Option<u64>,
}

impl ContextItem {
    pub fn new(mention: impl Into<String>) -> Self {
        Self {
            mention: mention.into(),
            status: ContextStatus::Pending,
            tokens: None,
        }
    }

    pub fn pending(mention: impl Into<String>) -> Self {
        Self::new(mention)
    }

    pub fn loading(mention: impl Into<String>) -> Self {
        Self {
            mention: mention.into(),
            status: ContextStatus::Loading,
            tokens: None,
        }
    }

    pub fn loaded(mention: impl Into<String>, tokens: u64) -> Self {
        Self {
            mention: mention.into(),
            status: ContextStatus::Loaded,
            tokens: Some(tokens),
        }
    }

    pub fn error(mention: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            mention: mention.into(),
            status: ContextStatus::Error(msg.into()),
            tokens: None,
        }
    }
}

/// Status of a context item
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextStatus {
    Pending,
    Loading,
    Loaded,
    Error(String),
}

/// Memory file info
#[derive(Debug, Clone)]
pub struct MemoryFile {
    pub name: String,
    pub kind: MemoryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Project,  // CLAUDE.md
    Session,  // .nika/memory.json
    System,   // System context
}

impl MemoryFile {
    pub fn project(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: MemoryKind::Project,
        }
    }

    pub fn session(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: MemoryKind::Session,
        }
    }
}

/// Runtime metrics for current turn
#[derive(Debug, Clone, Default)]
pub struct TurnMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

/// Mission Control Panel widget
pub struct MissionControlPanel<'a> {
    /// MCP servers
    mcp_servers: &'a [McpServerInfo],
    /// Context items (loaded @mentions)
    context_items: &'a [ContextItem],
    /// Memory files
    memory_files: &'a [MemoryFile],
    /// Conversation turn count
    conversation_turns: usize,
    /// Current verb being executed
    current_verb: CurrentVerb,
    /// Runtime metrics for current turn
    turn_metrics: TurnMetrics,
    /// Is panel focused
    focused: bool,
}

impl<'a> MissionControlPanel<'a> {
    pub fn new(mcp_servers: &'a [McpServerInfo]) -> Self {
        Self {
            mcp_servers,
            context_items: &[],
            memory_files: &[],
            conversation_turns: 0,
            current_verb: CurrentVerb::None,
            turn_metrics: TurnMetrics::default(),
            focused: false,
        }
    }

    pub fn context(mut self, items: &'a [ContextItem]) -> Self {
        self.context_items = items;
        self
    }

    pub fn memory(mut self, files: &'a [MemoryFile]) -> Self {
        self.memory_files = files;
        self
    }

    pub fn turns(mut self, count: usize) -> Self {
        self.conversation_turns = count;
        self
    }

    pub fn verb(mut self, verb: CurrentVerb) -> Self {
        self.current_verb = verb;
        self
    }

    pub fn metrics(mut self, metrics: TurnMetrics) -> Self {
        self.turn_metrics = metrics;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    fn render_mcp_section(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![Line::from(vec![
            Span::styled("🔌 ", Style::default()),
            Span::styled(
                "MCP SERVERS",
                Style::default()
                    .fg(COLOR_HEADER)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];

        if self.mcp_servers.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No servers connected",
                Style::default().fg(COLOR_MUTED),
            )));
        } else {
            for server in self.mcp_servers {
                let (icon, color) = match server.status {
                    McpStatus::Connected => ("●", COLOR_SUCCESS),
                    McpStatus::Connecting => ("◐", COLOR_WARNING),
                    McpStatus::Disconnected => ("○", COLOR_MUTED),
                    McpStatus::Error => ("✗", COLOR_ERROR),
                };

                let tool_count = server.tools.len();
                let tool_text = if tool_count > 0 {
                    format!(" ({} tools)", tool_count)
                } else {
                    String::new()
                };

                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(&server.name, Style::default().fg(Color::White)),
                    Span::styled(tool_text, Style::default().fg(COLOR_MUTED)),
                ]));
            }
        }

        let para = Paragraph::new(lines);
        para.render(area, buf);
    }

    fn render_context_section(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![Line::from(vec![
            Span::styled("📁 ", Style::default()),
            Span::styled(
                "CONTEXT",
                Style::default()
                    .fg(COLOR_HEADER)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];

        if self.context_items.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No context loaded",
                Style::default().fg(COLOR_MUTED),
            )));
        } else {
            for item in self.context_items {
                let (icon, color) = match &item.status {
                    ContextStatus::Pending => ("○", COLOR_MUTED),
                    ContextStatus::Loading => ("◐", COLOR_WARNING),
                    ContextStatus::Loaded => ("✓", COLOR_SUCCESS),
                    ContextStatus::Error(_) => ("✗", COLOR_ERROR),
                };

                let token_text = item
                    .tokens
                    .map(|t| format!(" ({:.1}k)", t as f64 / 1000.0))
                    .unwrap_or_default();

                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(&item.mention, Style::default().fg(COLOR_CYAN)),
                    Span::styled(token_text, Style::default().fg(COLOR_MUTED)),
                ]));
            }
        }

        let para = Paragraph::new(lines);
        para.render(area, buf);
    }

    fn render_memory_section(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![Line::from(vec![
            Span::styled("💾 ", Style::default()),
            Span::styled(
                "MEMORY",
                Style::default()
                    .fg(COLOR_HEADER)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];

        for file in self.memory_files {
            let kind_label = match file.kind {
                MemoryKind::Project => "(project)",
                MemoryKind::Session => "(session)",
                MemoryKind::System => "(system)",
            };

            lines.push(Line::from(vec![
                Span::raw("  • "),
                Span::styled(&file.name, Style::default().fg(Color::White)),
                Span::raw(" "),
                Span::styled(kind_label, Style::default().fg(COLOR_MUTED)),
            ]));
        }

        if self.conversation_turns > 0 {
            lines.push(Line::from(vec![
                Span::raw("  • "),
                Span::styled(
                    format!("{} conversation turns", self.conversation_turns),
                    Style::default().fg(Color::White),
                ),
            ]));
        }

        if self.memory_files.is_empty() && self.conversation_turns == 0 {
            lines.push(Line::from(Span::styled(
                "  No memory loaded",
                Style::default().fg(COLOR_MUTED),
            )));
        }

        let para = Paragraph::new(lines);
        para.render(area, buf);
    }

    fn render_runtime_section(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![Line::from(vec![
            Span::styled("⚡ ", Style::default()),
            Span::styled(
                "RUNTIME",
                Style::default()
                    .fg(COLOR_HEADER)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];

        // Current verb
        lines.push(Line::from(vec![
            Span::raw("  Current: "),
            Span::raw(self.current_verb.icon()),
            Span::raw(" "),
            Span::styled(
                self.current_verb.label(),
                Style::default().fg(self.current_verb.color()),
            ),
        ]));

        // Token counts
        let in_tokens = format_tokens(self.turn_metrics.input_tokens);
        let out_tokens = format_tokens(self.turn_metrics.output_tokens);
        lines.push(Line::from(vec![
            Span::raw("  In: "),
            Span::styled(in_tokens, Style::default().fg(COLOR_CYAN)),
            Span::raw(" │ Out: "),
            Span::styled(out_tokens, Style::default().fg(COLOR_VIOLET)),
        ]));

        // Cost
        let cost = format_cost(self.turn_metrics.cost_usd);
        lines.push(Line::from(vec![
            Span::raw("  Cost: "),
            Span::styled(cost, Style::default().fg(COLOR_WARNING)),
        ]));

        let para = Paragraph::new(lines);
        para.render(area, buf);
    }
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

impl Widget for MissionControlPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Border with title
        let border_style = if self.focused {
            Style::default().fg(COLOR_CYAN)
        } else {
            Style::default().fg(COLOR_SECTION)
        };

        let block = Block::default()
            .title(" 📊 MISSION CONTROL ")
            .title_style(Style::default().fg(COLOR_HEADER).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Split into 4 sections
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3 + self.mcp_servers.len().min(5) as u16), // MCP
                Constraint::Length(3 + self.context_items.len().min(5) as u16), // Context
                Constraint::Length(3 + self.memory_files.len().min(3) as u16 + if self.conversation_turns > 0 { 1 } else { 0 }), // Memory
                Constraint::Min(5), // Runtime
            ])
            .split(inner);

        self.render_mcp_section(sections[0], buf);
        self.render_context_section(sections[1], buf);
        self.render_memory_section(sections[2], buf);
        self.render_runtime_section(sections[3], buf);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_verb_display() {
        assert_eq!(CurrentVerb::Infer.icon(), "⚡");
        assert_eq!(CurrentVerb::Agent.label(), "agent");
        assert_eq!(CurrentVerb::None.label(), "idle");
    }

    #[test]
    fn test_context_item_constructors() {
        let pending = ContextItem::pending("@test");
        assert_eq!(pending.status, ContextStatus::Pending);

        let loaded = ContextItem::loaded("@test", 1000);
        assert_eq!(loaded.tokens, Some(1000));
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.001), "$0.0010");
        assert_eq!(format_cost(0.05), "$0.050");
        assert_eq!(format_cost(1.50), "$1.50");
    }
}
