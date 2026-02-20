# Chat UX Enrichment Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Transform Nika's Chat view into a rich, observable interface inspired by Claude Code's status line, with inline MCP call visualization, activity stack, and session metrics.

**Coordination:** This plan is designed to work alongside `2026-02-20-dag-ascii-visualizer.md` (running in parallel). We REUSE `VerbColor` from `theme.rs` — do NOT duplicate color definitions.

**Tech Stack:** Rust, ratatui, existing TUI infrastructure

---

## Architecture Overview

```
src/tui/
├── widgets/
│   ├── mod.rs                    # Add new exports
│   ├── session_context.rs        # NEW: Session metrics bar
│   ├── mcp_call_box.rs           # NEW: MCP call visualization
│   ├── infer_stream_box.rs       # NEW: Streaming inference display
│   ├── activity_stack.rs         # NEW: Hot/warm/cold activity
│   └── command_palette.rs        # NEW: ⌘K command search
├── views/
│   └── chat.rs                   # MODIFY: Integrate new widgets
└── theme.rs                      # REUSE: VerbColor (from DAG plan)
```

---

## Shared Types (Reused from DAG ASCII Plan)

These are already defined in `theme.rs` by the parallel DAG implementation:

```rust
// DO NOT REDEFINE - Import from theme.rs
pub enum VerbColor {
    Infer,   // Violet #8B5CF6 🧠
    Exec,    // Amber #F59E0B ⚡
    Fetch,   // Cyan #06B6D4 🌐
    Invoke,  // Emerald #10B981 🔧
    Agent,   // Rose #F43F5E 🤖
}
```

---

## Task 1: session_context.rs — Session Metrics Bar

**Files:**
- Create: `src/tui/widgets/session_context.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create session data struct**

```rust
//! Session Context Bar Widget
//!
//! Displays session metrics: tokens, cost, MCP status, active tasks.
//! Inspired by Claude Code's rich status line.

use std::time::{Duration, Instant};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// MCP server connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpStatus {
    /// Called < 30s ago, actively used
    Hot,
    /// Called < 5min ago, connection idle
    Warm,
    /// Not called recently
    Cold,
    /// Connection error
    Error,
}

impl McpStatus {
    pub fn indicator(&self) -> (&'static str, Color) {
        match self {
            Self::Hot => ("🟢", Color::Rgb(34, 197, 94)),   // Green
            Self::Warm => ("🟡", Color::Rgb(250, 204, 21)), // Yellow
            Self::Cold => ("⚪", Color::Rgb(156, 163, 175)), // Gray
            Self::Error => ("🔴", Color::Rgb(239, 68, 68)),  // Red
        }
    }
}

/// MCP server info for display
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub status: McpStatus,
    pub last_call: Option<Instant>,
}

/// Active operation in the activity stack
#[derive(Debug, Clone)]
pub struct ActiveOperation {
    pub id: String,
    pub verb: String,
    pub started: Instant,
    pub tokens_in: Option<u32>,
    pub tokens_out: Option<u32>,
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
}
```

**Step 2: Create widget for full display**

```rust
/// Session context bar widget (full version for Chat view)
pub struct SessionContextBar<'a> {
    context: &'a SessionContext,
    compact: bool,
}

impl<'a> SessionContextBar<'a> {
    pub fn new(context: &'a SessionContext) -> Self {
        Self {
            context,
            compact: false,
        }
    }

    /// Use compact single-line mode
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    fn render_progress_bar(&self, width: u16) -> String {
        let pct = self.context.usage_percent();
        let filled = ((pct / 100.0) * width as f64) as usize;
        let empty = width as usize - filled;
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

        let mcp = self.context.mcp_servers
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
        buf.set_string(area.x, area.y, &line, Style::default().fg(Color::Rgb(156, 163, 175)));
    }

    fn render_full(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 {
            return self.render_compact(area, buf);
        }

        let block = Block::default()
            .title(" 📊 SESSION CONTEXT ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(75, 85, 99)));

        let inner = block.inner(area);
        block.render(area, buf);

        // Line 1: Tokens
        let tokens_line = Line::from(vec![
            Span::styled("├─ 🧠 Tokens → ", Style::default().fg(Color::Rgb(107, 114, 128))),
            Span::styled(format!("💰 ${:.2}", self.context.total_cost), Style::default().fg(Color::Rgb(34, 197, 94))),
            Span::raw(" • "),
            Span::styled(
                format!("🧮 {}k/{}k", self.context.tokens_used / 1000, self.context.token_limit / 1000),
                Style::default().fg(Color::Rgb(147, 197, 253)),
            ),
            Span::raw(" • "),
            Span::styled(self.render_progress_bar(20), Style::default().fg(Color::Rgb(99, 102, 241))),
            Span::styled(format!(" ✦★ {:.0}%", self.context.usage_percent()), Style::default().fg(Color::Rgb(250, 204, 21))),
        ]);
        buf.set_line(inner.x, inner.y, &tokens_line, inner.width);

        // Line 2: Stats
        if inner.height > 1 {
            let stats_line = Line::from(vec![
                Span::styled("├─ 📈 Stats  → ", Style::default().fg(Color::Rgb(107, 114, 128))),
                Span::styled(format!("⏱ {}", self.context.format_duration()), Style::default().fg(Color::White)),
                Span::raw(" • "),
                Span::styled(
                    format!("📝 +{} -{}", self.context.files_modified.0, self.context.files_modified.1),
                    Style::default().fg(Color::Rgb(74, 222, 128)),
                ),
                Span::raw(" • "),
                Span::styled(
                    format!("💸 ${:.3}/min", self.context.cost_per_min()),
                    Style::default().fg(Color::Rgb(251, 191, 36)),
                ),
            ]);
            buf.set_line(inner.x, inner.y + 1, &stats_line, inner.width);
        }

        // Line 3: MCP servers
        if inner.height > 2 && !self.context.mcp_servers.is_empty() {
            let mut spans = vec![
                Span::styled("├─ 🔌 MCP    → ", Style::default().fg(Color::Rgb(107, 114, 128))),
            ];
            for (i, server) in self.context.mcp_servers.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" • "));
                }
                let (indicator, color) = server.status.indicator();
                let status_text = match server.status {
                    McpStatus::Hot => "hot",
                    McpStatus::Warm => "warm",
                    McpStatus::Cold => "cold",
                    McpStatus::Error => "error",
                };
                spans.push(Span::styled(indicator, Style::default().fg(color)));
                spans.push(Span::styled(format!(" {} ({})", server.name, status_text), Style::default().fg(color)));
            }
            buf.set_line(inner.x, inner.y + 2, &Line::from(spans), inner.width);
        }

        // Line 4: Active operations
        if inner.height > 3 && !self.context.active_ops.is_empty() {
            let mut spans = vec![
                Span::styled("└─ 🎯 Active → ", Style::default().fg(Color::Rgb(107, 114, 128))),
            ];
            for (i, op) in self.context.active_ops.iter().take(3).enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" • "));
                }
                let verb_icon = match op.verb.as_str() {
                    "infer" => "🧠",
                    "exec" => "⚡",
                    "fetch" => "🌐",
                    "invoke" => "🔧",
                    "agent" => "🤖",
                    _ => "●",
                };
                spans.push(Span::styled(
                    format!("{} {}:{}", verb_icon, op.verb, op.id),
                    Style::default().fg(Color::Rgb(167, 139, 250)),
                ));
            }
            buf.set_line(inner.x, inner.y + 3, &Line::from(spans), inner.width);
        }
    }
}
```

**Step 3: Add tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_context_defaults() {
        let ctx = SessionContext::new();
        assert_eq!(ctx.token_limit, 200_000);
        assert_eq!(ctx.usage_percent(), 0.0);
    }

    #[test]
    fn test_usage_percent() {
        let mut ctx = SessionContext::new();
        ctx.tokens_used = 50_000;
        ctx.token_limit = 200_000;
        assert_eq!(ctx.usage_percent(), 25.0);
    }

    #[test]
    fn test_mcp_status_indicators() {
        assert_eq!(McpStatus::Hot.indicator().0, "🟢");
        assert_eq!(McpStatus::Warm.indicator().0, "🟡");
        assert_eq!(McpStatus::Cold.indicator().0, "⚪");
        assert_eq!(McpStatus::Error.indicator().0, "🔴");
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
}
```

**Step 4: Update mod.rs, run tests, commit**

```bash
cargo test --features tui session_context
git add src/tui/widgets/session_context.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add SessionContextBar widget with token/cost/MCP metrics

Inspired by Claude Code status line:
- Token usage with progress bar
- Cost tracking (total + per minute)
- MCP server status (hot/warm/cold)
- Active operations stack

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: mcp_call_box.rs — MCP Call Visualization

**Files:**
- Create: `src/tui/widgets/mcp_call_box.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create MCP call data struct**

```rust
//! MCP Call Box Widget
//!
//! Renders inline MCP tool call visualization with params, result, and timing.

use std::time::Duration;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Status of an MCP call
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpCallStatus {
    Running,
    Success,
    Failed,
}

/// Data for rendering an MCP call box
#[derive(Debug, Clone)]
pub struct McpCallData {
    /// Tool name (e.g., "novanet_describe")
    pub tool: String,
    /// MCP server name
    pub server: String,
    /// Input parameters (JSON string, truncated)
    pub params: String,
    /// Result (JSON string, truncated) - None if still running
    pub result: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// Call duration
    pub duration: Duration,
    /// Call status
    pub status: McpCallStatus,
    /// Whether result is expanded
    pub expanded: bool,
    /// Animation frame for spinner
    pub frame: u8,
}

impl McpCallData {
    pub fn new(tool: impl Into<String>, server: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            server: server.into(),
            params: String::new(),
            result: None,
            error: None,
            duration: Duration::ZERO,
            status: McpCallStatus::Running,
            expanded: false,
            frame: 0,
        }
    }

    pub fn with_params(mut self, params: impl Into<String>) -> Self {
        self.params = params.into();
        self
    }

    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = Some(result.into());
        self.status = McpCallStatus::Success;
        self
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self.status = McpCallStatus::Failed;
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    fn status_indicator(&self) -> (&'static str, Color) {
        match self.status {
            McpCallStatus::Running => {
                let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                let idx = (self.frame as usize) % spinners.len();
                (spinners[idx], Color::Rgb(250, 204, 21)) // Yellow
            }
            McpCallStatus::Success => ("✅", Color::Rgb(34, 197, 94)), // Green
            McpCallStatus::Failed => ("❌", Color::Rgb(239, 68, 68)),   // Red
        }
    }
}
```

**Step 2: Implement Widget**

```rust
/// MCP call box widget
pub struct McpCallBox<'a> {
    data: &'a McpCallData,
}

impl<'a> McpCallBox<'a> {
    pub fn new(data: &'a McpCallData) -> Self {
        Self { data }
    }

    /// Calculate required height
    pub fn required_height(&self) -> u16 {
        let mut height = 3; // borders + header
        if !self.data.params.is_empty() {
            height += 1;
        }
        if self.data.result.is_some() || self.data.error.is_some() {
            height += 1;
        }
        if self.data.expanded {
            height += 3; // Extra lines for expanded content
        }
        height
    }
}

impl Widget for McpCallBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 3 {
            return;
        }

        let (status_char, status_color) = self.data.status_indicator();

        // Border color: emerald for invoke
        let border_color = Color::Rgb(16, 185, 129); // Emerald
        let border_style = Style::default().fg(border_color);

        // Top border with title
        let duration_str = format!("{:.1}s", self.data.duration.as_secs_f64());
        let title = format!(
            "╭─ 🔧 MCP CALL: {} ─{:─>width$} {} ─╮",
            self.data.tool,
            "─",
            status_char,
            width = area.width.saturating_sub(self.data.tool.len() as u16 + 25) as usize
        );

        // Truncate title if too long
        let title = if title.len() > area.width as usize {
            format!("╭─ 🔧 {} {} ─╮", self.data.tool, status_char)
        } else {
            title
        };

        buf.set_string(area.x, area.y, &title, border_style);

        // Duration on the right
        let dur_x = area.x + area.width - duration_str.len() as u16 - 3;
        buf.set_string(dur_x, area.y, &duration_str, Style::default().fg(Color::DarkGray));

        // Side borders and content
        let mut y = area.y + 1;

        // Params line
        if !self.data.params.is_empty() && y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let params_display = if self.data.params.len() > (area.width - 15) as usize {
                format!("{}...", &self.data.params[..(area.width - 18) as usize])
            } else {
                self.data.params.clone()
            };
            buf.set_string(
                area.x + 2,
                y,
                &format!("📥 params: {}", params_display),
                Style::default().fg(Color::Rgb(156, 163, 175)),
            );
            y += 1;
        }

        // Result or error line
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            if let Some(ref error) = self.data.error {
                let error_display = if error.len() > (area.width - 15) as usize {
                    format!("{}...", &error[..(area.width - 18) as usize])
                } else {
                    error.clone()
                };
                buf.set_string(
                    area.x + 2,
                    y,
                    &format!("❌ Error: {}", error_display),
                    Style::default().fg(Color::Rgb(239, 68, 68)),
                );
            } else if let Some(ref result) = self.data.result {
                let result_display = if result.len() > (area.width - 15) as usize {
                    format!("{}...", &result[..(area.width - 18) as usize])
                } else {
                    result.clone()
                };
                buf.set_string(
                    area.x + 2,
                    y,
                    &format!("📤 result: {}", result_display),
                    Style::default().fg(Color::Rgb(34, 197, 94)),
                );
            } else {
                buf.set_string(
                    area.x + 2,
                    y,
                    "⏳ Running...",
                    Style::default().fg(Color::Rgb(250, 204, 21)),
                );
            }
            y += 1;
        }

        // Bottom border
        let bottom = "╰".to_string() + &"─".repeat((area.width - 2) as usize) + "╯";
        buf.set_string(area.x, area.y + area.height - 1, &bottom, border_style);
    }
}
```

**Step 3: Tests and commit**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_call_creation() {
        let call = McpCallData::new("novanet_describe", "novanet")
            .with_params(r#"{ "entity": "qr-code" }"#)
            .with_duration(Duration::from_millis(1234));

        assert_eq!(call.tool, "novanet_describe");
        assert_eq!(call.status, McpCallStatus::Running);
    }

    #[test]
    fn test_mcp_call_success() {
        let call = McpCallData::new("novanet_describe", "novanet")
            .with_result(r#"{ "display_name": "QR Code" }"#);

        assert_eq!(call.status, McpCallStatus::Success);
        assert!(call.result.is_some());
    }

    #[test]
    fn test_mcp_call_failure() {
        let call = McpCallData::new("novanet_traverse", "novanet")
            .with_error("Entity not found");

        assert_eq!(call.status, McpCallStatus::Failed);
        assert!(call.error.is_some());
    }
}
```

```bash
cargo test --features tui mcp_call_box
git add src/tui/widgets/mcp_call_box.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add McpCallBox widget for inline MCP visualization

Shows MCP tool calls with:
- Tool name and server
- Input params (truncated)
- Result or error (with status colors)
- Duration and animated spinner

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: infer_stream_box.rs — Streaming Inference Display

**Files:**
- Create: `src/tui/widgets/infer_stream_box.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create inference data struct**

```rust
//! Infer Stream Box Widget
//!
//! Renders streaming LLM inference with token counter and progress.

use std::time::Duration;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Status of inference
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferStatus {
    Running,
    Complete,
    Failed,
}

/// Data for rendering an inference stream box
#[derive(Debug, Clone)]
pub struct InferStreamData {
    /// Model name
    pub model: String,
    /// Tokens input
    pub tokens_in: u32,
    /// Tokens output (so far)
    pub tokens_out: u32,
    /// Max tokens
    pub max_tokens: u32,
    /// Temperature
    pub temperature: f32,
    /// Duration
    pub duration: Duration,
    /// Status
    pub status: InferStatus,
    /// Streaming content
    pub content: String,
    /// Animation frame
    pub frame: u8,
}

impl InferStreamData {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            tokens_in: 0,
            tokens_out: 0,
            max_tokens: 2000,
            temperature: 0.7,
            duration: Duration::ZERO,
            status: InferStatus::Running,
            content: String::new(),
            frame: 0,
        }
    }

    pub fn with_tokens(mut self, input: u32, output: u32) -> Self {
        self.tokens_in = input;
        self.tokens_out = output;
        self
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    fn progress_percent(&self) -> f64 {
        if self.max_tokens == 0 {
            0.0
        } else {
            (self.tokens_out as f64 / self.max_tokens as f64) * 100.0
        }
    }
}

/// Infer stream box widget
pub struct InferStreamBox<'a> {
    data: &'a InferStreamData,
    max_content_lines: u16,
}

impl<'a> InferStreamBox<'a> {
    pub fn new(data: &'a InferStreamData) -> Self {
        Self {
            data,
            max_content_lines: 6,
        }
    }

    pub fn max_lines(mut self, lines: u16) -> Self {
        self.max_content_lines = lines;
        self
    }

    pub fn required_height(&self) -> u16 {
        // borders + header + stats + separator + content lines + progress
        4 + self.max_content_lines.min(self.data.content.lines().count() as u16)
    }
}

impl Widget for InferStreamBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 6 {
            return;
        }

        // Border color: violet for infer
        let border_color = Color::Rgb(139, 92, 246); // Violet
        let border_style = Style::default().fg(border_color);

        let (status_char, _) = match self.data.status {
            InferStatus::Running => {
                let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                let idx = (self.data.frame as usize) % spinners.len();
                (spinners[idx], Color::Rgb(250, 204, 21))
            }
            InferStatus::Complete => ("✅", Color::Rgb(34, 197, 94)),
            InferStatus::Failed => ("❌", Color::Rgb(239, 68, 68)),
        };

        // Top border
        let duration_str = format!("{:.1}s", self.data.duration.as_secs_f64());
        let title = format!(
            "╭─ 🧠 INFER: {} {} {} ─╮",
            self.data.model,
            "─".repeat((area.width as usize).saturating_sub(self.data.model.len() + 25)),
            status_char
        );
        buf.set_string(area.x, area.y, &title[..title.len().min(area.width as usize)], border_style);
        buf.set_string(
            area.x + area.width - duration_str.len() as u16 - 3,
            area.y,
            &duration_str,
            Style::default().fg(Color::DarkGray),
        );

        let mut y = area.y + 1;

        // Stats line: tokens
        buf.set_string(area.x, y, "│", border_style);
        buf.set_string(area.x + area.width - 1, y, "│", border_style);
        let status_text = if self.data.status == InferStatus::Running {
            "(streaming...)"
        } else {
            ""
        };
        buf.set_string(
            area.x + 2,
            y,
            &format!(
                "📊 tokens: {} in → {} out {}",
                self.data.tokens_in, self.data.tokens_out, status_text
            ),
            Style::default().fg(Color::Rgb(156, 163, 175)),
        );
        y += 1;

        // Separator
        buf.set_string(area.x, y, "│", border_style);
        buf.set_string(area.x + area.width - 1, y, "│", border_style);
        let separator = "─".repeat((area.width - 4) as usize);
        buf.set_string(area.x + 2, y, &separator, Style::default().fg(Color::Rgb(55, 65, 81)));
        y += 1;

        // Content lines
        let content_lines: Vec<&str> = self.data.content.lines().collect();
        let start = content_lines.len().saturating_sub(self.max_content_lines as usize);
        for line in content_lines.iter().skip(start).take(self.max_content_lines as usize) {
            if y >= area.y + area.height - 2 {
                break;
            }
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let display_line = if line.len() > (area.width - 4) as usize {
                &line[..(area.width - 4) as usize]
            } else {
                line
            };
            buf.set_string(area.x + 2, y, display_line, Style::default().fg(Color::White));
            y += 1;
        }

        // Cursor if streaming
        if self.data.status == InferStatus::Running && y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            buf.set_string(
                area.x + 2 + (content_lines.last().map(|l| l.len()).unwrap_or(0) as u16 % (area.width - 4)),
                y - 1,
                "█",
                Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK),
            );
        }

        // Progress bar at bottom
        let progress_y = area.y + area.height - 2;
        buf.set_string(area.x, progress_y, "│", border_style);
        buf.set_string(area.x + area.width - 1, progress_y, "│", border_style);

        let bar_width = (area.width - 25) as usize;
        let filled = ((self.data.progress_percent() / 100.0) * bar_width as f64) as usize;
        let bar = format!(
            "[{}{}] {}/{} tokens",
            "░".repeat(filled),
            " ".repeat(bar_width.saturating_sub(filled)),
            self.data.tokens_out,
            self.data.max_tokens
        );
        buf.set_string(area.x + 2, progress_y, &bar, Style::default().fg(Color::Rgb(107, 114, 128)));

        // Bottom border
        let bottom = "╰".to_string() + &"─".repeat((area.width - 2) as usize) + "╯";
        buf.set_string(area.x, area.y + area.height - 1, &bottom, border_style);
    }
}
```

**Step 2: Tests and commit**

```bash
cargo test --features tui infer_stream_box
git add src/tui/widgets/infer_stream_box.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add InferStreamBox for streaming LLM visualization

Displays streaming inference with:
- Model name and status spinner
- Token counter (in/out)
- Live content with blinking cursor
- Progress bar toward max_tokens

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: activity_stack.rs — Hot/Warm/Cold Activity

**Files:**
- Create: `src/tui/widgets/activity_stack.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create activity stack**

```rust
//! Activity Stack Widget
//!
//! Shows hot (executing), warm (recent), and queued operations.

use std::time::{Duration, Instant};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

/// Activity temperature
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityTemp {
    Hot,    // Currently executing
    Warm,   // Recently completed (< 30s)
    Queued, // Waiting for dependencies
}

/// Activity item
#[derive(Debug, Clone)]
pub struct ActivityItem {
    pub id: String,
    pub verb: String,
    pub temp: ActivityTemp,
    pub started: Option<Instant>,
    pub duration: Option<Duration>,
    pub tokens: Option<(u32, u32)>, // (in, out)
    pub waiting_on: Option<String>,
    pub detail: Option<String>,
    pub frame: u8,
}

impl ActivityItem {
    pub fn hot(id: impl Into<String>, verb: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Hot,
            started: Some(Instant::now()),
            duration: None,
            tokens: None,
            waiting_on: None,
            detail: None,
            frame: 0,
        }
    }

    pub fn warm(id: impl Into<String>, verb: impl Into<String>, duration: Duration) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Warm,
            started: None,
            duration: Some(duration),
            tokens: None,
            waiting_on: None,
            detail: None,
            frame: 0,
        }
    }

    pub fn queued(id: impl Into<String>, verb: impl Into<String>, waiting_on: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verb: verb.into(),
            temp: ActivityTemp::Queued,
            started: None,
            duration: None,
            tokens: None,
            waiting_on: Some(waiting_on.into()),
            detail: None,
            frame: 0,
        }
    }

    fn verb_icon(&self) -> &'static str {
        match self.verb.as_str() {
            "infer" => "🧠",
            "exec" => "⚡",
            "fetch" => "🌐",
            "invoke" => "🔧",
            "agent" => "🤖",
            _ => "●",
        }
    }

    fn verb_color(&self) -> Color {
        match self.verb.as_str() {
            "infer" => Color::Rgb(139, 92, 246),  // Violet
            "exec" => Color::Rgb(245, 158, 11),   // Amber
            "fetch" => Color::Rgb(6, 182, 212),   // Cyan
            "invoke" => Color::Rgb(16, 185, 129), // Emerald
            "agent" => Color::Rgb(244, 63, 94),   // Rose
            _ => Color::Gray,
        }
    }
}

/// Activity stack widget
pub struct ActivityStack<'a> {
    items: &'a [ActivityItem],
    frame: u8,
}

impl<'a> ActivityStack<'a> {
    pub fn new(items: &'a [ActivityItem]) -> Self {
        Self { items, frame: 0 }
    }

    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }
}

impl Widget for ActivityStack<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 {
            return;
        }

        let block = Block::default()
            .title(" 🎯 ACTIVITY STACK ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(75, 85, 99)));

        let inner = block.inner(area);
        block.render(area, buf);

        let hot: Vec<_> = self.items.iter().filter(|i| i.temp == ActivityTemp::Hot).collect();
        let warm: Vec<_> = self.items.iter().filter(|i| i.temp == ActivityTemp::Warm).collect();
        let queued: Vec<_> = self.items.iter().filter(|i| i.temp == ActivityTemp::Queued).collect();

        let mut y = inner.y;

        // Hot section
        if !hot.is_empty() && y < inner.y + inner.height {
            buf.set_string(
                inner.x,
                y,
                "🔥 HOT (executing now)",
                Style::default().fg(Color::Rgb(251, 146, 60)).add_modifier(Modifier::BOLD),
            );
            y += 1;

            for item in hot.iter().take(2) {
                if y >= inner.y + inner.height {
                    break;
                }
                let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                let spinner = spinners[(self.frame as usize) % spinners.len()];
                let elapsed = item.started.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);

                let line = format!(
                    "└── {} {}:{}  {} {:.1}s",
                    item.verb_icon(),
                    item.verb,
                    item.id,
                    spinner,
                    elapsed
                );
                buf.set_string(inner.x, y, &line, Style::default().fg(item.verb_color()));

                if let Some((t_in, t_out)) = item.tokens {
                    let tokens_str = format!("  {}/{} tokens", t_out, t_in);
                    buf.set_string(
                        inner.x + line.len() as u16,
                        y,
                        &tokens_str,
                        Style::default().fg(Color::DarkGray),
                    );
                }
                y += 1;
            }
        }

        // Warm section
        if !warm.is_empty() && y < inner.y + inner.height {
            y += 1; // spacing
            buf.set_string(
                inner.x,
                y,
                "🟡 WARM (recently completed)",
                Style::default().fg(Color::Rgb(250, 204, 21)),
            );
            y += 1;

            for item in warm.iter().take(3) {
                if y >= inner.y + inner.height {
                    break;
                }
                let dur = item.duration.map(|d| format!("{:.1}s", d.as_secs_f64())).unwrap_or_default();
                let detail = item.detail.as_deref().unwrap_or("");
                let detail_display = if detail.len() > 30 {
                    format!("{}...", &detail[..27])
                } else {
                    detail.to_string()
                };

                buf.set_string(
                    inner.x,
                    y,
                    &format!("├── {} {}:{}", item.verb_icon(), item.verb, item.id),
                    Style::default().fg(Color::DarkGray),
                );
                buf.set_string(
                    inner.x + 30,
                    y,
                    &format!("✅ {}  {}", dur, detail_display),
                    Style::default().fg(Color::Rgb(34, 197, 94)),
                );
                y += 1;
            }
        }

        // Queued section
        if !queued.is_empty() && y < inner.y + inner.height {
            y += 1; // spacing
            buf.set_string(
                inner.x,
                y,
                "⚪ QUEUED (waiting)",
                Style::default().fg(Color::Rgb(156, 163, 175)),
            );
            y += 1;

            for item in queued.iter().take(3) {
                if y >= inner.y + inner.height {
                    break;
                }
                let waiting = item.waiting_on.as_deref().unwrap_or("dependencies");
                buf.set_string(
                    inner.x,
                    y,
                    &format!(
                        "├── {} {}:{}  ○ waiting on {}",
                        item.verb_icon(),
                        item.verb,
                        item.id,
                        waiting
                    ),
                    Style::default().fg(Color::Rgb(107, 114, 128)),
                );
                y += 1;
            }
        }
    }
}
```

**Step 2: Tests and commit**

```bash
cargo test --features tui activity_stack
git add src/tui/widgets/activity_stack.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add ActivityStack widget with hot/warm/queued sections

Visual activity monitor showing:
- 🔥 HOT: Currently executing with spinner
- 🟡 WARM: Recently completed with duration
- ⚪ QUEUED: Waiting with dependency info

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: command_palette.rs — ⌘K Command Search

**Files:**
- Create: `src/tui/widgets/command_palette.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create command palette**

```rust
//! Command Palette Widget
//!
//! Fuzzy command search overlay inspired by VS Code ⌘K.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget},
};

/// A command in the palette
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    /// Command ID
    pub id: String,
    /// Display label
    pub label: String,
    /// Description
    pub description: String,
    /// Keyboard shortcut
    pub shortcut: Option<String>,
    /// Icon
    pub icon: &'static str,
    /// Category
    pub category: String,
}

impl PaletteCommand {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: description.into(),
            shortcut: None,
            icon: "▶",
            category: "General".to_string(),
        }
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Check if command matches query (fuzzy)
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query_lower = query.to_lowercase();
        self.label.to_lowercase().contains(&query_lower)
            || self.description.to_lowercase().contains(&query_lower)
            || self.id.to_lowercase().contains(&query_lower)
    }
}

/// Default commands
pub fn default_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand::new("run", "Run Workflow", "Execute the current workflow file")
            .with_shortcut("⌘⏎")
            .with_icon("▶")
            .with_category("Run"),
        PaletteCommand::new("run_task", "Run Task", "Execute a single task")
            .with_shortcut("⌘⇧G")
            .with_icon("🔷")
            .with_category("Run"),
        PaletteCommand::new("run_monitor", "Run with Monitor", "Execute and open TUI monitor")
            .with_shortcut("⌘M")
            .with_icon("📊")
            .with_category("Run"),
        PaletteCommand::new("dry_run", "Dry Run", "Validate DAG without executing")
            .with_shortcut("⌘D")
            .with_icon("🧪")
            .with_category("Run"),
        PaletteCommand::new("validate", "Validate Workflow", "Check YAML and schema")
            .with_shortcut("⌘V")
            .with_icon("✅")
            .with_category("Edit"),
        PaletteCommand::new("chat", "Open Chat", "Switch to chat view")
            .with_shortcut("⌘C")
            .with_icon("💬")
            .with_category("View"),
        PaletteCommand::new("monitor", "Open Monitor", "Switch to monitor view")
            .with_shortcut("⌘M")
            .with_icon("📊")
            .with_category("View"),
        PaletteCommand::new("home", "Open Home", "Switch to file browser")
            .with_shortcut("⌘H")
            .with_icon("🏠")
            .with_category("View"),
        PaletteCommand::new("help", "Help", "Show help documentation")
            .with_shortcut("?")
            .with_icon("❓")
            .with_category("Help"),
    ]
}

/// Command palette state
pub struct CommandPaletteState {
    /// Search query
    pub query: String,
    /// Selected index
    pub selected: usize,
    /// All commands
    pub commands: Vec<PaletteCommand>,
    /// Filtered commands
    pub filtered: Vec<usize>,
    /// Recently used (command IDs)
    pub recent: Vec<String>,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        let commands = default_commands();
        let filtered = (0..commands.len()).collect();
        Self {
            query: String::new(),
            selected: 0,
            commands,
            filtered,
            recent: Vec::new(),
        }
    }
}

impl CommandPaletteState {
    pub fn update_filter(&mut self) {
        self.filtered = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| cmd.matches(&self.query))
            .map(|(i, _)| i)
            .collect();
        self.selected = 0;
    }

    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.filtered.len() - 1);
        }
    }

    pub fn selected_command(&self) -> Option<&PaletteCommand> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.commands.get(i))
    }

    pub fn input_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filter();
    }
}

/// Command palette widget
pub struct CommandPalette<'a> {
    state: &'a CommandPaletteState,
}

impl<'a> CommandPalette<'a> {
    pub fn new(state: &'a CommandPaletteState) -> Self {
        Self { state }
    }
}

impl Widget for CommandPalette<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the palette
        let palette_width = 60.min(area.width.saturating_sub(10));
        let palette_height = 15.min(area.height.saturating_sub(6));

        let x = area.x + (area.width - palette_width) / 2;
        let y = area.y + 3;

        let palette_area = Rect {
            x,
            y,
            width: palette_width,
            height: palette_height,
        };

        // Clear background
        Clear.render(palette_area, buf);

        // Draw border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(99, 102, 241)))
            .style(Style::default().bg(Color::Rgb(17, 24, 39)));
        let inner = block.inner(palette_area);
        block.render(palette_area, buf);

        // Search input
        let input_line = Line::from(vec![
            Span::styled("🔍 > ", Style::default().fg(Color::Rgb(156, 163, 175))),
            Span::styled(&self.state.query, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK)),
        ]);
        buf.set_line(inner.x + 1, inner.y, &input_line, inner.width - 2);

        // Separator
        let sep = "─".repeat((inner.width - 2) as usize);
        buf.set_string(
            inner.x + 1,
            inner.y + 1,
            &sep,
            Style::default().fg(Color::Rgb(55, 65, 81)),
        );

        // Command list
        let list_area = Rect {
            x: inner.x,
            y: inner.y + 2,
            width: inner.width,
            height: inner.height.saturating_sub(2),
        };

        for (i, &cmd_idx) in self.state.filtered.iter().enumerate() {
            if i >= list_area.height as usize {
                break;
            }
            let cmd = &self.state.commands[cmd_idx];
            let is_selected = i == self.state.selected;

            let bg = if is_selected {
                Color::Rgb(55, 65, 81)
            } else {
                Color::Rgb(17, 24, 39)
            };

            // Clear line background
            for x in list_area.x..(list_area.x + list_area.width) {
                buf.get_mut(x, list_area.y + i as u16).set_bg(bg);
            }

            // Icon and label
            let label_style = if is_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(229, 231, 235))
            };

            buf.set_string(
                list_area.x + 2,
                list_area.y + i as u16,
                cmd.icon,
                Style::default(),
            );
            buf.set_string(
                list_area.x + 5,
                list_area.y + i as u16,
                &cmd.label,
                label_style,
            );

            // Shortcut on the right
            if let Some(ref shortcut) = cmd.shortcut {
                let shortcut_x = list_area.x + list_area.width - shortcut.len() as u16 - 3;
                buf.set_string(
                    shortcut_x,
                    list_area.y + i as u16,
                    shortcut,
                    Style::default().fg(Color::Rgb(107, 114, 128)),
                );
            }

            // Description on next conceptual line (if space)
            // For now, skip to keep it compact
        }

        // Footer hint
        if inner.height > 10 {
            buf.set_string(
                inner.x + 2,
                inner.y + inner.height - 1,
                "↑↓ Navigate  ⏎ Select  Esc Cancel",
                Style::default().fg(Color::Rgb(107, 114, 128)),
            );
        }
    }
}
```

**Step 2: Tests and commit**

```bash
cargo test --features tui command_palette
git add src/tui/widgets/command_palette.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): add CommandPalette widget with fuzzy search

⌘K-style command search with:
- Fuzzy filtering
- Keyboard shortcuts display
- Recently used tracking
- Centered overlay design

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Integrate into chat.rs

**Files:**
- Modify: `src/tui/views/chat.rs`

**Step 1: Add imports and state**

```rust
use crate::tui::widgets::{
    SessionContextBar, SessionContext, McpServerInfo, McpStatus,
    McpCallBox, McpCallData, McpCallStatus,
    InferStreamBox, InferStreamData, InferStatus,
    ActivityStack, ActivityItem,
    CommandPalette, CommandPaletteState,
};
```

**Step 2: Add session context to ChatState**

Add `session_context: SessionContext` field to track metrics.

**Step 3: Add activity items tracking**

Add `activity_items: Vec<ActivityItem>` for the activity stack.

**Step 4: Add command palette state**

Add `command_palette: Option<CommandPaletteState>` for ⌘K overlay.

**Step 5: Update render method**

```rust
fn render(&self, frame: &mut Frame, area: Rect) {
    // Layout: Context Bar | Messages + Sidebar | Input
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Session context bar
            Constraint::Min(10),    // Messages area
            Constraint::Length(3),  // Input
            Constraint::Length(1),  // Command hints
        ])
        .split(area);

    // 1. Session Context Bar (full version)
    SessionContextBar::new(&self.session_context).render(chunks[0], frame.buffer_mut());

    // 2. Messages + Sidebar (existing layout)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(chunks[1]);

    // Left: Messages with inline MCP/Infer boxes
    self.render_messages(frame, main_chunks[0]);

    // Right: Activity Stack
    ActivityStack::new(&self.activity_items)
        .frame(self.frame)
        .render(main_chunks[1], frame.buffer_mut());

    // 3. Input
    self.render_input(frame, chunks[2]);

    // 4. Command hints
    self.render_hints(frame, chunks[3]);

    // 5. Command Palette overlay (if active)
    if let Some(ref palette_state) = self.command_palette {
        CommandPalette::new(palette_state).render(area, frame.buffer_mut());
    }
}
```

**Step 6: Handle ⌘K keybinding**

In key handling:
```rust
KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    self.command_palette = Some(CommandPaletteState::default());
}
```

**Step 7: Test and commit**

```bash
cargo test --features tui chat
cargo run --features tui -- tui examples/
# Press Ctrl+K to open command palette
git add src/tui/views/chat.rs
git commit -m "feat(tui): integrate session context, activity stack, and command palette

Chat view now includes:
- Session metrics bar with tokens/cost/MCP status
- Activity stack showing hot/warm/queued operations
- ⌘K command palette overlay
- Inline MCP and Infer visualization boxes

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
# - [ ] Session context bar shows tokens, cost, duration
# - [ ] MCP servers show hot/warm/cold status
# - [ ] Activity stack updates during execution
# - [ ] Ctrl+K opens command palette
# - [ ] Fuzzy search filters commands
# - [ ] MCP calls render inline in messages
# - [ ] Infer streaming shows progress bar
```

---

## Summary

| Task | Files | Description |
|------|-------|-------------|
| 1 | session_context.rs | Token/cost/MCP metrics bar |
| 2 | mcp_call_box.rs | Inline MCP call visualization |
| 3 | infer_stream_box.rs | Streaming inference display |
| 4 | activity_stack.rs | Hot/warm/queued activity |
| 5 | command_palette.rs | ⌘K fuzzy command search |
| 6 | chat.rs | Integration of all widgets |

**Coordination with DAG ASCII Plan:**
- REUSES `VerbColor` from theme.rs (do not duplicate)
- Same verb icons: 🧠⚡🌐🔧🤖
- Same color palette (Tailwind)
- Consistent border styles
