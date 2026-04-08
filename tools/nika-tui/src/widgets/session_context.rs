// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Session Context Bar Widget
//!
//! Displays session metrics: tokens, cost, MCP status, active tasks.
//! Inspired by Claude Code's rich status line.

use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::theme::{Theme, VerbColor};

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks when no theme)
// ═══════════════════════════════════════════════════════════════════════════

const DEFAULT_HOT_COLOR: Color = Color::Rgb(34, 197, 94); // green
const DEFAULT_WARM_COLOR: Color = Color::Rgb(250, 204, 21); // yellow
const DEFAULT_CONNECTED_COLOR: Color = Color::Rgb(59, 130, 246); // blue
const DEFAULT_COLD_COLOR: Color = Color::Rgb(156, 163, 175); // gray
const DEFAULT_ERROR_COLOR: Color = Color::Rgb(239, 68, 68); // red
const DEFAULT_MUTED_COLOR: Color = Color::Rgb(107, 114, 128); // gray-500
const DEFAULT_BORDER_COLOR: Color = Color::Rgb(75, 85, 99); // gray-600
const DEFAULT_SUCCESS_COLOR: Color = Color::Rgb(34, 197, 94); // green
const DEFAULT_INFO_COLOR: Color = Color::Rgb(147, 197, 253); // blue-300
const DEFAULT_HIGHLIGHT_COLOR: Color = Color::Rgb(99, 102, 241); // indigo
const DEFAULT_WARNING_COLOR: Color = Color::Rgb(250, 204, 21); // yellow
const DEFAULT_FILES_COLOR: Color = Color::Rgb(74, 222, 128); // green-400
const DEFAULT_COST_COLOR: Color = Color::Rgb(251, 191, 36); // amber
const DEFAULT_ACTIVE_COLOR: Color = Color::Rgb(167, 139, 250); // violet

/// MCP server connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpStatus {
    /// Called < 30s ago, actively used
    Hot,
    /// Called < 5min ago, connection idle
    Warm,
    /// Connected but idle
    Connected,
    /// Not called recently
    #[default]
    Cold,
    /// Connection error
    Error,
}

impl McpStatus {
    /// Get indicator emoji and default color
    pub fn indicator(&self) -> (&'static str, Color) {
        match self {
            Self::Hot => ("🟢", DEFAULT_HOT_COLOR),
            Self::Warm => ("🟡", DEFAULT_WARM_COLOR),
            Self::Connected => ("🔵", DEFAULT_CONNECTED_COLOR),
            Self::Cold => ("⚪", DEFAULT_COLD_COLOR),
            Self::Error => ("🔴", DEFAULT_ERROR_COLOR),
        }
    }

    /// Get indicator color with theme support
    pub fn indicator_color_with_theme(&self, theme: Option<&Theme>) -> Color {
        match self {
            Self::Hot => theme.map(|t| t.status_success).unwrap_or(DEFAULT_HOT_COLOR),
            Self::Warm => theme
                .map(|t| t.status_running)
                .unwrap_or(DEFAULT_WARM_COLOR),
            Self::Connected => theme
                .map(|t| t.highlight)
                .unwrap_or(DEFAULT_CONNECTED_COLOR),
            Self::Cold => theme.map(|t| t.text_muted).unwrap_or(DEFAULT_COLD_COLOR),
            Self::Error => theme
                .map(|t| t.status_failed)
                .unwrap_or(DEFAULT_ERROR_COLOR),
        }
    }

    /// Get indicator emoji only
    pub fn indicator_emoji(&self) -> &'static str {
        match self {
            Self::Hot => "🟢",
            Self::Warm => "🟡",
            Self::Connected => "🔵",
            Self::Cold => "⚪",
            Self::Error => "🔴",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Hot => "hot",
            Self::Warm => "warm",
            Self::Connected => "ready",
            Self::Cold => "cold",
            Self::Error => "error",
        }
    }

    /// Determine status from last call time
    pub fn from_last_call(last_call: Option<Instant>) -> Self {
        match last_call {
            Some(t) => {
                let elapsed = t.elapsed();
                if elapsed < Duration::from_secs(30) {
                    Self::Hot
                } else if elapsed < Duration::from_secs(300) {
                    Self::Warm
                } else {
                    Self::Cold
                }
            }
            None => Self::Cold,
        }
    }
}

/// MCP server info for display
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub status: McpStatus,
    pub last_call: Option<Instant>,
    pub call_count: u32,
    /// Last measured latency in milliseconds
    pub latency_ms: Option<u64>,
}

impl McpServerInfo {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: McpStatus::Cold,
            last_call: None,
            call_count: 0,
            latency_ms: None,
        }
    }

    pub fn with_status(mut self, status: McpStatus) -> Self {
        self.status = status;
        self
    }

    /// Update status after a call
    pub fn record_call(&mut self) {
        self.last_call = Some(Instant::now());
        self.status = McpStatus::Hot;
        self.call_count += 1;
    }

    /// Mark server as connected
    pub fn mark_connected(&mut self) {
        // Only upgrade from Cold to Connected, don't downgrade Hot/Warm
        if self.status == McpStatus::Cold {
            self.status = McpStatus::Connected;
        }
    }

    /// Update server with ping result
    pub fn update_from_ping(&mut self, connected: bool, latency_ms: u64) {
        self.latency_ms = Some(latency_ms);
        if connected {
            self.status = McpStatus::Connected;
        } else {
            self.status = McpStatus::Error;
        }
    }

    /// Mark server as errored
    pub fn mark_error(&mut self) {
        self.status = McpStatus::Error;
    }

    /// Update status based on elapsed time
    pub fn update_status(&mut self) {
        // Don't downgrade Connected to Cold based on time alone
        match self.status {
            McpStatus::Hot | McpStatus::Warm => {
                self.status = McpStatus::from_last_call(self.last_call);
                // If we're going Cold but were previously active, go to Connected
                if self.status == McpStatus::Cold && self.call_count > 0 {
                    self.status = McpStatus::Connected;
                }
            }
            McpStatus::Connected | McpStatus::Cold | McpStatus::Error => {
                // Don't change status based on time for these states
            }
        }
    }
}

/// Active operation in the activity stack
#[derive(Debug, Clone)]
pub struct ActiveOperation {
    pub id: String,
    pub verb: String,
    pub started: Instant,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
}

impl ActiveOperation {
    pub fn new(id: impl Into<String>, verb: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            started: Instant::now(),
            tokens_in: None,
            tokens_out: None,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn verb_icon(&self) -> &'static str {
        VerbColor::from_verb(&self.verb).icon()
    }
}

/// Session context data
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    /// Total cost in dollars
    pub total_cost: f64,
    /// Tokens used
    pub tokens_used: u64,
    /// Token limit
    pub token_limit: u64,
    /// Session start time
    pub started: Option<Instant>,
    /// Files modified (+additions, -deletions)
    pub files_modified: (u32, u32),
    /// MCP servers
    pub mcp_servers: Vec<McpServerInfo>,
    /// Active operations
    pub active_ops: Vec<ActiveOperation>,
}

impl SessionContext {
    pub fn new() -> Self {
        Self {
            token_limit: 200_000,
            started: Some(Instant::now()),
            ..Default::default()
        }
    }

    /// Get context usage percentage
    pub fn usage_percent(&self) -> f64 {
        if self.token_limit == 0 {
            0.0
        } else {
            (self.tokens_used as f64 / self.token_limit as f64) * 100.0
        }
    }

    /// Get session duration
    pub fn duration(&self) -> Duration {
        self.started.map(|s| s.elapsed()).unwrap_or_default()
    }

    /// Get cost per minute
    pub fn cost_per_min(&self) -> f64 {
        let mins = self.duration().as_secs_f64() / 60.0;
        if mins > 0.0 {
            self.total_cost / mins
        } else {
            0.0
        }
    }

    /// Format duration as "Xm Ys"
    pub fn format_duration(&self) -> String {
        let secs = self.duration().as_secs();
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{}m {:02}s", mins, secs)
    }

    /// Add tokens and update cost
    pub fn add_tokens(&mut self, input: u64, output: u64) {
        self.tokens_used += input + output;
        // Approximate cost: $3/M input, $15/M output (Claude Sonnet pricing)
        self.total_cost +=
            (input as f64 * 3.0 / 1_000_000.0) + (output as f64 * 15.0 / 1_000_000.0);
    }

    /// Add or update MCP server
    pub fn add_mcp_server(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !self.mcp_servers.iter().any(|s| s.name == name) {
            self.mcp_servers.push(McpServerInfo::new(name));
        }
    }

    /// Record MCP call
    pub fn record_mcp_call(&mut self, server: &str) {
        if let Some(s) = self.mcp_servers.iter_mut().find(|s| s.name == server) {
            s.record_call();
        }
    }

    /// Update all MCP statuses
    pub fn update_mcp_statuses(&mut self) {
        for server in &mut self.mcp_servers {
            server.update_status();
        }
    }

    /// Start an operation
    pub fn start_operation(&mut self, id: impl Into<String>, verb: impl Into<String>) {
        self.active_ops.push(ActiveOperation::new(id, verb));
    }

    /// Complete an operation
    pub fn complete_operation(&mut self, id: &str) {
        self.active_ops.retain(|op| op.id != id);
    }
}

/// Session context bar widget (full version for Chat view)
pub struct SessionContextBar<'a> {
    context: &'a SessionContext,
    compact: bool,
    /// Optional theme for colors
    theme: Option<&'a Theme>,
}

impl<'a> SessionContextBar<'a> {
    pub fn new(context: &'a SessionContext) -> Self {
        Self {
            context,
            compact: false,
            theme: None,
        }
    }

    /// Set the theme for colors
    pub fn with_theme(mut self, theme: &'a Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Use compact single-line mode
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    fn render_progress_bar(&self, width: usize) -> String {
        let pct = self.context.usage_percent();
        let filled = ((pct / 100.0) * width as f64) as usize;
        let empty = width.saturating_sub(filled);
        format!("[{}{}]", "▓".repeat(filled), "░".repeat(empty))
    }
}

impl Widget for SessionContextBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.compact {
            self.render_compact(area, buf);
        } else {
            self.render_full(area, buf);
        }
    }
}

impl SessionContextBar<'_> {
    fn render_compact(&self, area: Rect, buf: &mut Buffer) {
        // Single line: 💰 $2.47 │ 🧮 47k/200k [▓▓▓░░░] 24% │ 🔌 novanet🟢 │ ⏱ 4:12
        if area.height < 1 {
            return;
        }

        let cost = format!("💰 ${:.2}", self.context.total_cost);
        let tokens = format!(
            "🧮 {}k/{}k {} {:.0}%",
            self.context.tokens_used / 1000,
            self.context.token_limit / 1000,
            self.render_progress_bar(10),
            self.context.usage_percent()
        );

        let mcp = self
            .context
            .mcp_servers
            .iter()
            .take(1)
            .map(|s| {
                let (ind, _) = s.status.indicator();
                format!("🔌 {}{}", s.name, ind)
            })
            .next()
            .unwrap_or_else(|| "🔌 --".to_string());

        let time = format!("⏱ {}", self.context.format_duration());

        let line = format!("{} │ {} │ {} │ {}", cost, tokens, mcp, time);
        let muted_color = self
            .theme
            .map(|t| t.text_muted)
            .unwrap_or(DEFAULT_COLD_COLOR);
        buf.set_string(area.x, area.y, &line, Style::default().fg(muted_color));
    }

    fn render_full(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 {
            return self.render_compact(area, buf);
        }

        // Extract theme colors with fallbacks
        let border_color = self
            .theme
            .map(|t| t.border_normal)
            .unwrap_or(DEFAULT_BORDER_COLOR);
        let muted_color = self
            .theme
            .map(|t| t.text_muted)
            .unwrap_or(DEFAULT_MUTED_COLOR);
        let success_color = self
            .theme
            .map(|t| t.status_success)
            .unwrap_or(DEFAULT_SUCCESS_COLOR);
        let info_color = self
            .theme
            .map(|t| t.highlight)
            .unwrap_or(DEFAULT_INFO_COLOR);
        let highlight_color = self
            .theme
            .map(|t| t.highlight)
            .unwrap_or(DEFAULT_HIGHLIGHT_COLOR);
        let warning_color = self
            .theme
            .map(|t| t.status_running)
            .unwrap_or(DEFAULT_WARNING_COLOR);
        let text_color = self.theme.map(|t| t.text_primary).unwrap_or(Color::White);
        let files_color = self
            .theme
            .map(|t| t.status_success)
            .unwrap_or(DEFAULT_FILES_COLOR);
        let cost_color = self
            .theme
            .map(|t| t.status_running)
            .unwrap_or(DEFAULT_COST_COLOR);
        let active_color = self
            .theme
            .map(|t| t.highlight)
            .unwrap_or(DEFAULT_ACTIVE_COLOR);

        let block = Block::default()
            .title(" 📊 SESSION CONTEXT ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let inner = block.inner(area);
        block.render(area, buf);

        // Line 1: Tokens
        let tokens_line = Line::from(vec![
            Span::styled("├─ 🧠 Tokens → ", Style::default().fg(muted_color)),
            Span::styled(
                format!("💰 ${:.2}", self.context.total_cost),
                Style::default().fg(success_color),
            ),
            Span::raw(" • "),
            Span::styled(
                format!(
                    "🧮 {}k/{}k",
                    self.context.tokens_used / 1000,
                    self.context.token_limit / 1000
                ),
                Style::default().fg(info_color),
            ),
            Span::raw(" • "),
            Span::styled(
                self.render_progress_bar(20),
                Style::default().fg(highlight_color),
            ),
            Span::styled(
                format!(" ✦★ {:.0}%", self.context.usage_percent()),
                Style::default().fg(warning_color),
            ),
        ]);
        buf.set_line(inner.x, inner.y, &tokens_line, inner.width);

        // Line 2: Stats
        if inner.height > 1 {
            let stats_line = Line::from(vec![
                Span::styled("├─ 📈 Stats  → ", Style::default().fg(muted_color)),
                Span::styled(
                    format!("⏱ {}", self.context.format_duration()),
                    Style::default().fg(text_color),
                ),
                Span::raw(" • "),
                Span::styled(
                    format!(
                        "📝 +{} -{}",
                        self.context.files_modified.0, self.context.files_modified.1
                    ),
                    Style::default().fg(files_color),
                ),
                Span::raw(" • "),
                Span::styled(
                    format!("💸 ${:.3}/min", self.context.cost_per_min()),
                    Style::default().fg(cost_color),
                ),
            ]);
            buf.set_line(inner.x, inner.y + 1, &stats_line, inner.width);
        }

        // Line 3: MCP servers
        if inner.height > 2 && !self.context.mcp_servers.is_empty() {
            let mut spans = vec![Span::styled(
                "├─ 🔌 MCP    → ",
                Style::default().fg(muted_color),
            )];
            for (i, server) in self.context.mcp_servers.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" • "));
                }
                let indicator = server.status.indicator_emoji();
                let color = server.status.indicator_color_with_theme(self.theme);
                spans.push(Span::styled(indicator, Style::default().fg(color)));
                spans.push(Span::styled(
                    format!(" {} ({})", server.name, server.status.label()),
                    Style::default().fg(color),
                ));
            }
            buf.set_line(inner.x, inner.y + 2, &Line::from(spans), inner.width);
        }

        // Line 4: Active operations
        if inner.height > 3 && !self.context.active_ops.is_empty() {
            let mut spans = vec![Span::styled(
                "└─ 🎯 Active → ",
                Style::default().fg(muted_color),
            )];
            for (i, op) in self.context.active_ops.iter().take(3).enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" • "));
                }
                spans.push(Span::styled(
                    format!("{} {}:{}", op.verb_icon(), op.verb, op.id),
                    Style::default().fg(active_color),
                ));
            }
            buf.set_line(inner.x, inner.y + 3, &Line::from(spans), inner.width);
        } else if inner.height > 3 {
            // Show "no active operations" when empty
            buf.set_line(
                inner.x,
                inner.y + 3,
                &Line::from(vec![Span::styled(
                    "└─ 🎯 Active → (none)",
                    Style::default().fg(muted_color),
                )]),
                inner.width,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_context_defaults() {
        let ctx = SessionContext::new();
        assert_eq!(ctx.token_limit, 200_000);
        assert_eq!(ctx.usage_percent(), 0.0);
        assert!(ctx.started.is_some());
    }

    #[test]
    fn test_usage_percent() {
        let mut ctx = SessionContext::new();
        ctx.tokens_used = 50_000;
        ctx.token_limit = 200_000;
        assert_eq!(ctx.usage_percent(), 25.0);
    }

    #[test]
    fn test_usage_percent_zero_limit() {
        let mut ctx = SessionContext::new();
        ctx.token_limit = 0;
        assert_eq!(ctx.usage_percent(), 0.0);
    }

    #[test]
    fn test_mcp_status_indicators() {
        assert_eq!(McpStatus::Hot.indicator().0, "🟢");
        assert_eq!(McpStatus::Warm.indicator().0, "🟡");
        assert_eq!(McpStatus::Cold.indicator().0, "⚪");
        assert_eq!(McpStatus::Error.indicator().0, "🔴");
    }

    #[test]
    fn test_mcp_status_labels() {
        assert_eq!(McpStatus::Hot.label(), "hot");
        assert_eq!(McpStatus::Warm.label(), "warm");
        assert_eq!(McpStatus::Cold.label(), "cold");
        assert_eq!(McpStatus::Error.label(), "error");
    }

    #[test]
    fn test_mcp_status_from_last_call() {
        // Hot: < 30s
        let recent = Some(Instant::now());
        assert_eq!(McpStatus::from_last_call(recent), McpStatus::Hot);

        // Cold: None
        assert_eq!(McpStatus::from_last_call(None), McpStatus::Cold);
    }

    #[test]
    fn test_mcp_server_info() {
        let mut server = McpServerInfo::new("novanet");
        assert_eq!(server.name, "novanet");
        assert_eq!(server.status, McpStatus::Cold);
        assert_eq!(server.call_count, 0);

        server.record_call();
        assert_eq!(server.status, McpStatus::Hot);
        assert_eq!(server.call_count, 1);
        assert!(server.last_call.is_some());
    }

    #[test]
    fn test_format_duration() {
        let ctx = SessionContext {
            started: Some(Instant::now() - Duration::from_secs(125)),
            ..Default::default()
        };
        let dur = ctx.format_duration();
        assert!(dur.contains("2m"));
    }

    #[test]
    fn test_add_tokens() {
        let mut ctx = SessionContext::new();
        ctx.add_tokens(1000, 500);
        assert_eq!(ctx.tokens_used, 1500);
        assert!(ctx.total_cost > 0.0);
    }

    #[test]
    fn test_add_mcp_server() {
        let mut ctx = SessionContext::new();
        ctx.add_mcp_server("novanet");
        ctx.add_mcp_server("novanet"); // Duplicate should not add
        ctx.add_mcp_server("firecrawl");

        assert_eq!(ctx.mcp_servers.len(), 2);
    }

    #[test]
    fn test_record_mcp_call() {
        let mut ctx = SessionContext::new();
        ctx.add_mcp_server("novanet");
        ctx.record_mcp_call("novanet");

        assert_eq!(ctx.mcp_servers[0].status, McpStatus::Hot);
        assert_eq!(ctx.mcp_servers[0].call_count, 1);
    }

    #[test]
    fn test_active_operations() {
        let mut ctx = SessionContext::new();
        ctx.start_operation("task1", "infer");
        ctx.start_operation("task2", "exec");

        assert_eq!(ctx.active_ops.len(), 2);

        ctx.complete_operation("task1");
        assert_eq!(ctx.active_ops.len(), 1);
        assert_eq!(ctx.active_ops[0].id, "task2");
    }

    #[test]
    fn test_active_operation_verb_icon() {
        // Canonical icons from CLAUDE.md (via VerbColor::from_verb().icon())
        let op = ActiveOperation::new("test", "infer");
        assert_eq!(op.verb_icon(), "⚡"); // LLM generation

        let op = ActiveOperation::new("test", "exec");
        assert_eq!(op.verb_icon(), "📟"); // Shell command

        let op = ActiveOperation::new("test", "invoke");
        assert_eq!(op.verb_icon(), "🔌"); // MCP tool
    }

    #[test]
    fn test_progress_bar_rendering() {
        let ctx = SessionContext {
            tokens_used: 50_000,
            token_limit: 200_000,
            ..Default::default()
        };
        let bar = SessionContextBar::new(&ctx);
        let rendered = bar.render_progress_bar(20);

        // 25% = 5 filled out of 20
        assert!(rendered.contains("▓"));
        assert!(rendered.contains("░"));
        // Unicode chars have different byte lengths, check char count instead
        assert_eq!(rendered.chars().count(), 22); // 20 + 2 brackets
    }

    #[test]
    fn test_cost_per_min() {
        let mut ctx = SessionContext::new();
        ctx.total_cost = 1.0;
        // After some time, cost_per_min should be > 0
        // Can't easily test time-based without mocking
        let cpm = ctx.cost_per_min();
        assert!(cpm >= 0.0);
    }

    #[test]
    fn test_session_context_bar_compact() {
        let ctx = SessionContext::new();
        let bar = SessionContextBar::new(&ctx).compact();
        assert!(bar.compact);
    }
}
