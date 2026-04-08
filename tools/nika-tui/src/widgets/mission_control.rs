// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

use super::{ActivityItem, ActivityTemp, McpServerInfo, McpStatus};
use crate::theme::{Theme, VerbColor};
use crate::utils::format_number_compact;

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks when no theme provided)
// ═══════════════════════════════════════════════════════════════════════════

const COLOR_HEADER: Color = Color::Rgb(250, 204, 21); // Gold
const COLOR_SECTION: Color = Color::Rgb(147, 161, 161); // Gray
const COLOR_SUCCESS: Color = Color::Rgb(133, 153, 0); // Green
const COLOR_WARNING: Color = Color::Rgb(181, 137, 0); // Yellow
const COLOR_ERROR: Color = Color::Rgb(220, 50, 47); // Red
const COLOR_MUTED: Color = Color::Rgb(88, 110, 117); // Muted
const COLOR_CYAN: Color = Color::Rgb(42, 161, 152); // Cyan
const COLOR_VIOLET: Color = Color::Rgb(108, 113, 196); // Violet
const COLOR_ORANGE: Color = Color::Rgb(251, 146, 60); // Orange

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
    Spawn,
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
            CurrentVerb::Spawn => "🐤",
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
            CurrentVerb::Spawn => "spawn",
        }
    }

    /// Get color for this verb (delegates to VerbColor for consistency)
    pub fn color(&self) -> Color {
        match self {
            CurrentVerb::None => COLOR_MUTED,
            CurrentVerb::Infer => VerbColor::Infer.rgb(),
            CurrentVerb::Exec => VerbColor::Exec.rgb(),
            CurrentVerb::Fetch => VerbColor::Fetch.rgb(),
            CurrentVerb::Invoke => VerbColor::Invoke.rgb(),
            CurrentVerb::Agent => VerbColor::Agent.rgb(),
            CurrentVerb::Spawn => VerbColor::Spawn.rgb(),
        }
    }

    /// Get color using theme (for None, uses theme muted color)
    pub fn color_with_theme(&self, theme: Option<&Theme>) -> Color {
        match self {
            CurrentVerb::None => theme.map(|t| t.text_muted).unwrap_or(COLOR_MUTED),
            _ => self.color(), // VerbColor already provides consistent colors
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
    Project, // CLAUDE.md
    Session, // .nika/memory.json
    System,  // System context
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
    /// Activity items (hot/warm/queued)
    activities: &'a [ActivityItem],
    /// Animation frame for spinners
    frame: u8,
    /// Optional theme for colors
    theme: Option<&'a Theme>,
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
            activities: &[],
            frame: 0,
            theme: None,
        }
    }

    /// Set the theme for colors
    pub fn with_theme(mut self, theme: &'a Theme) -> Self {
        self.theme = Some(theme);
        self
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

    /// Set activity items (hot/warm/queued tasks)
    pub fn activities(mut self, activities: &'a [ActivityItem]) -> Self {
        self.activities = activities;
        self
    }

    /// Set animation frame for spinners
    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // THEME-AWARE COLOR HELPERS
    // ═══════════════════════════════════════════════════════════════════════════

    fn color_header(&self) -> Color {
        self.theme.map(|t| t.status_running).unwrap_or(COLOR_HEADER)
    }

    fn color_section(&self) -> Color {
        self.theme.map(|t| t.border_normal).unwrap_or(COLOR_SECTION)
    }

    fn color_success(&self) -> Color {
        self.theme
            .map(|t| t.status_success)
            .unwrap_or(COLOR_SUCCESS)
    }

    fn color_warning(&self) -> Color {
        self.theme
            .map(|t| t.status_running)
            .unwrap_or(COLOR_WARNING)
    }

    fn color_error(&self) -> Color {
        self.theme.map(|t| t.status_failed).unwrap_or(COLOR_ERROR)
    }

    fn color_muted(&self) -> Color {
        self.theme.map(|t| t.text_muted).unwrap_or(COLOR_MUTED)
    }

    fn color_cyan(&self) -> Color {
        self.theme.map(|t| t.highlight).unwrap_or(COLOR_CYAN)
    }

    fn color_violet(&self) -> Color {
        self.theme
            .map(|_| VerbColor::Infer.rgb())
            .unwrap_or(COLOR_VIOLET)
    }

    fn color_orange(&self) -> Color {
        self.theme.map(|t| t.status_running).unwrap_or(COLOR_ORANGE)
    }

    fn color_text(&self) -> Color {
        self.theme.map(|t| t.text_primary).unwrap_or(Color::White)
    }

    fn render_mcp_section(&self, area: Rect, buf: &mut Buffer) {
        let header = self.color_header();
        let muted = self.color_muted();
        let success = self.color_success();
        let warning = self.color_warning();
        let error = self.color_error();
        let text = self.color_text();

        let mut lines = vec![Line::from(vec![
            Span::styled("🔌 ", Style::default()),
            Span::styled(
                "MCP SERVERS",
                Style::default().fg(header).add_modifier(Modifier::BOLD),
            ),
        ])];

        if self.mcp_servers.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No servers connected",
                Style::default().fg(muted),
            )));
        } else {
            for server in self.mcp_servers {
                // Map McpStatus to icon and color
                let (icon, color) = match server.status {
                    McpStatus::Hot => ("●", success),       // Active
                    McpStatus::Warm => ("◐", warning),      // Idle
                    McpStatus::Connected => ("●", success), // Connected
                    McpStatus::Cold => ("○", muted),        // Not used recently
                    McpStatus::Error => ("✗", error),       // Error
                };

                // Show call count as activity indicator
                let activity_text = if server.call_count > 0 {
                    format!(" ({} calls)", server.call_count)
                } else {
                    String::new()
                };

                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(&server.name, Style::default().fg(text)),
                    Span::styled(activity_text, Style::default().fg(muted)),
                ]));
            }
        }

        let para = Paragraph::new(lines);
        para.render(area, buf);
    }

    fn render_context_section(&self, area: Rect, buf: &mut Buffer) {
        let header = self.color_header();
        let muted = self.color_muted();
        let success = self.color_success();
        let warning = self.color_warning();
        let error = self.color_error();
        let cyan = self.color_cyan();

        let mut lines = vec![Line::from(vec![
            Span::styled("📁 ", Style::default()),
            Span::styled(
                "CONTEXT",
                Style::default().fg(header).add_modifier(Modifier::BOLD),
            ),
        ])];

        if self.context_items.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No context loaded",
                Style::default().fg(muted),
            )));
        } else {
            for item in self.context_items {
                let (icon, color) = match &item.status {
                    ContextStatus::Pending => ("○", muted),
                    ContextStatus::Loading => ("◐", warning),
                    ContextStatus::Loaded => ("✓", success),
                    ContextStatus::Error(_) => ("✗", error),
                };

                let token_text = item
                    .tokens
                    .map(|t| format!(" ({:.1}k)", t as f64 / 1000.0))
                    .unwrap_or_default();

                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(&item.mention, Style::default().fg(cyan)),
                    Span::styled(token_text, Style::default().fg(muted)),
                ]));
            }
        }

        let para = Paragraph::new(lines);
        para.render(area, buf);
    }

    fn render_memory_section(&self, area: Rect, buf: &mut Buffer) {
        let header = self.color_header();
        let muted = self.color_muted();
        let text = self.color_text();

        let mut lines = vec![Line::from(vec![
            Span::styled("💾 ", Style::default()),
            Span::styled(
                "MEMORY",
                Style::default().fg(header).add_modifier(Modifier::BOLD),
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
                Span::styled(&file.name, Style::default().fg(text)),
                Span::raw(" "),
                Span::styled(kind_label, Style::default().fg(muted)),
            ]));
        }

        if self.conversation_turns > 0 {
            lines.push(Line::from(vec![
                Span::raw("  • "),
                Span::styled(
                    format!("{} conversation turns", self.conversation_turns),
                    Style::default().fg(text),
                ),
            ]));
        }

        if self.memory_files.is_empty() && self.conversation_turns == 0 {
            lines.push(Line::from(Span::styled(
                "  No memory loaded",
                Style::default().fg(muted),
            )));
        }

        let para = Paragraph::new(lines);
        para.render(area, buf);
    }

    fn render_runtime_section(&self, area: Rect, buf: &mut Buffer) {
        let header = self.color_header();
        let cyan = self.color_cyan();
        let violet = self.color_violet();
        let warning = self.color_warning();

        let mut lines = vec![Line::from(vec![
            Span::styled("⚡ ", Style::default()),
            Span::styled(
                "RUNTIME",
                Style::default().fg(header).add_modifier(Modifier::BOLD),
            ),
        ])];

        // Current verb (uses VerbColor via CurrentVerb.color_with_theme())
        lines.push(Line::from(vec![
            Span::raw("  Current: "),
            Span::raw(self.current_verb.icon()),
            Span::raw(" "),
            Span::styled(
                self.current_verb.label(),
                Style::default().fg(self.current_verb.color_with_theme(self.theme)),
            ),
        ]));

        // Token counts
        let in_tokens = format_number_compact(self.turn_metrics.input_tokens);
        let out_tokens = format_number_compact(self.turn_metrics.output_tokens);
        lines.push(Line::from(vec![
            Span::raw("  In: "),
            Span::styled(in_tokens, Style::default().fg(cyan)),
            Span::raw(" │ Out: "),
            Span::styled(out_tokens, Style::default().fg(violet)),
        ]));

        // Cost
        let cost = format_cost(self.turn_metrics.cost_usd);
        lines.push(Line::from(vec![
            Span::raw("  Cost: "),
            Span::styled(cost, Style::default().fg(warning)),
        ]));

        let para = Paragraph::new(lines);
        para.render(area, buf);
    }

    /// Render activity section (hot/warm/queued tasks)
    fn render_activity_section(&self, area: Rect, buf: &mut Buffer) {
        let header = self.color_header();
        let muted = self.color_muted();
        let orange = self.color_orange();
        let warning = self.color_warning();
        let text = self.color_text();

        // Group activities by temperature
        let hot: Vec<_> = self
            .activities
            .iter()
            .filter(|a| a.temp == ActivityTemp::Hot)
            .collect();
        let warm: Vec<_> = self
            .activities
            .iter()
            .filter(|a| a.temp == ActivityTemp::Warm)
            .collect();
        let queued: Vec<_> = self
            .activities
            .iter()
            .filter(|a| a.temp == ActivityTemp::Queued)
            .collect();

        let mut lines = vec![Line::from(vec![
            Span::styled("🎯 ", Style::default()),
            Span::styled(
                "ACTIVITY",
                Style::default().fg(header).add_modifier(Modifier::BOLD),
            ),
        ])];

        if self.activities.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No active tasks",
                Style::default().fg(muted),
            )));
        } else {
            // Hot (executing) - orange spinner
            for item in hot.iter().take(3) {
                let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                let spinner = spinners[(self.frame as usize) % spinners.len()];
                let duration = item
                    .started
                    .map(|s| format!(" ({:.1}s)", s.elapsed().as_secs_f64()))
                    .unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(spinner, Style::default().fg(orange)),
                    Span::raw(" "),
                    Span::styled(&item.verb, Style::default().fg(text)),
                    Span::styled(duration, Style::default().fg(muted)),
                ]));
            }

            // Warm (recent) - yellow/warning checkmark
            for item in warm.iter().take(2) {
                let duration = item
                    .duration
                    .map(|d| format!(" ({:.1}s)", d.as_secs_f64()))
                    .unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("✓", Style::default().fg(warning)),
                    Span::raw(" "),
                    Span::styled(&item.verb, Style::default().fg(muted)),
                    Span::styled(duration, Style::default().fg(muted)),
                ]));
            }

            // Queued (waiting) - muted gray
            for item in queued.iter().take(2) {
                let waiting = item
                    .waiting_on
                    .as_ref()
                    .map(|w| format!(" (→ {})", w))
                    .unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("○", Style::default().fg(muted)),
                    Span::raw(" "),
                    Span::styled(&item.verb, Style::default().fg(muted)),
                    Span::styled(waiting, Style::default().fg(muted)),
                ]));
            }

            // Summary if more items
            let remaining = self.activities.len().saturating_sub(7);
            if remaining > 0 {
                lines.push(Line::from(Span::styled(
                    format!("  +{} more", remaining),
                    Style::default().fg(muted),
                )));
            }
        }

        let para = Paragraph::new(lines);
        para.render(area, buf);
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
        // Minimum height guard to prevent rendering issues
        // Need at least: border (2) + 1 section header + 1 content line = 4 lines minimum
        if area.height < 4 {
            // Too small to render - just show border with ellipsis
            let block = Block::default()
                .title(" 📊 ... ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray));
            block.render(area, buf);
            return;
        }

        // Get theme-aware colors
        let header = self.color_header();
        let cyan = self.color_cyan();
        let section = self.color_section();

        // Border with title
        let border_style = if self.focused {
            Style::default().fg(cyan)
        } else {
            Style::default().fg(section)
        };

        let block = Block::default()
            .title(" 📊 MISSION CONTROL ")
            .title_style(Style::default().fg(header).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Split into 5 sections
        let activity_lines = if self.activities.is_empty() {
            2 // header + "No active tasks"
        } else {
            2 + self.activities.len().min(7) // header + up to 7 items
        };

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3 + self.mcp_servers.len().min(5) as u16), // MCP
                Constraint::Length(3 + self.context_items.len().min(5) as u16), // Context
                Constraint::Length(
                    3 + self.memory_files.len().min(3) as u16
                        + if self.conversation_turns > 0 { 1 } else { 0 },
                ), // Memory
                Constraint::Length(activity_lines as u16),                    // Activity
                Constraint::Min(5),                                           // Runtime
            ])
            .split(inner);

        self.render_mcp_section(sections[0], buf);
        self.render_context_section(sections[1], buf);
        self.render_memory_section(sections[2], buf);
        self.render_activity_section(sections[3], buf);
        self.render_runtime_section(sections[4], buf);
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
    fn test_format_number_compact() {
        assert_eq!(format_number_compact(500), "500");
        assert_eq!(format_number_compact(1500), "1.5K");
        assert_eq!(format_number_compact(1_500_000), "1.5M");
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.001), "$0.0010");
        assert_eq!(format_cost(0.05), "$0.050");
        assert_eq!(format_cost(1.50), "$1.50");
    }
}
