// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ExecBox Widget
//!
//! Shell command execution box with stdout/stderr separation.
//! Shows command, output streams, exit code, and timing.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{ListItem, Widget},
};

use super::{exit, BoxState, RenderMode, StreamingContext, VerbColor};
use crate::tokens::compat;
use crate::unicode::display_width;

/// ExecBox data and rendering
#[derive(Debug, Clone)]
pub struct ExecBox {
    /// Shell command
    pub command: String,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Exit code (None if still running)
    pub exit_code: Option<i32>,
    /// Process ID
    pub pid: Option<u32>,
    /// Working directory
    pub cwd: Option<String>,
    /// Execution state
    pub state: BoxState,
    /// Is stdout section expanded
    pub expanded_stdout: bool,
    /// Is stderr section expanded
    pub expanded_stderr: bool,
    /// Cached stdout line count (updated on append, avoids O(n) scan per frame)
    stdout_line_count: usize,
    /// Cached stderr line count (updated on append, avoids O(n) scan per frame)
    stderr_line_count: usize,
    /// Pulse intensity for border animation (0.0-1.0)
    pub pulse_intensity: f32,
    /// Render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
}

impl ExecBox {
    /// Create a new ExecBox
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            pid: None,
            cwd: None,
            state: BoxState::default(),
            expanded_stdout: false,
            expanded_stderr: false,
            stdout_line_count: 0,
            stderr_line_count: 0,
            pulse_intensity: 0.0,
            render_mode: RenderMode::default(),
        }
    }

    /// Set the state
    pub fn with_state(mut self, state: BoxState) -> Self {
        self.state = state;
        self
    }

    /// Set stdout
    pub fn with_stdout(mut self, stdout: impl Into<String>) -> Self {
        let s: String = stdout.into();
        self.stdout_line_count = s.bytes().filter(|&b| b == b'\n').count();
        self.stdout = s;
        self
    }

    /// Set stderr
    pub fn with_stderr(mut self, stderr: impl Into<String>) -> Self {
        let s: String = stderr.into();
        self.stderr_line_count = s.bytes().filter(|&b| b == b'\n').count();
        self.stderr = s;
        self
    }

    /// Set exit code
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    /// Set PID
    pub fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    /// Set working directory
    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set render mode
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Set pulse intensity for border animation (clamped to 0.0-1.0)
    pub fn with_pulse_intensity(mut self, intensity: f32) -> Self {
        self.pulse_intensity = intensity.clamp(0.0, 1.0);
        self
    }

    /// Append to stdout (streaming)
    pub fn append_stdout(&mut self, text: &str) {
        self.stdout_line_count += text.bytes().filter(|&b| b == b'\n').count();
        self.stdout.push_str(text);
    }

    /// Append to stderr (streaming)
    pub fn append_stderr(&mut self, text: &str) {
        self.stderr_line_count += text.bytes().filter(|&b| b == b'\n').count();
        self.stderr.push_str(text);
    }

    /// Toggle stdout expansion
    pub fn toggle_stdout(&mut self) {
        self.expanded_stdout = !self.expanded_stdout;
    }

    /// Toggle stderr expansion
    pub fn toggle_stderr(&mut self) {
        self.expanded_stderr = !self.expanded_stderr;
    }

    /// Calculate required height
    pub fn required_height(&self) -> u16 {
        let mut height: u16 = 4; // Header + command + footer + bottom border

        // Stdout lines
        if !self.stdout.is_empty() {
            height += 2; // Header + content
            if self.expanded_stdout {
                height += self.stdout_line_count.min(10) as u16;
            }
        }

        // Stderr lines
        if !self.stderr.is_empty() {
            height += 2; // Header + content
            if self.expanded_stderr {
                height += self.stderr_line_count.min(5) as u16;
            }
        }

        height
    }

    /// Truncate string preserving UTF-8, using display width for terminal columns
    fn truncate(s: &str, max_len: usize) -> String {
        let width = display_width(s);
        if width > max_len && max_len > 3 {
            let target = max_len - 3;
            let mut current = 0;
            let mut result = String::new();
            for c in s.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                if current + cw > target {
                    break;
                }
                result.push(c);
                current += cw;
            }
            format!("{}...", result)
        } else {
            s.to_string()
        }
    }

    /// Get exit code display text
    fn exit_display(&self) -> String {
        match self.exit_code {
            Some(0) => "exit: 0 ✓".to_string(),
            Some(code) => format!("exit: {} ✗", code),
            None => "exit: ?".to_string(),
        }
    }

    /// Convert to list items for scrollable List widget
    pub fn to_list_items(&self, _ctx: &StreamingContext) -> Vec<ListItem<'static>> {
        let verb = VerbColor::Exec;
        let verb_color = verb.rgb();
        let dim_style = Style::default().fg(compat::SLATE_500);

        // Compact mode: single line summary
        if self.render_mode == RenderMode::Compact {
            let status = self.state.icon();
            let suffix = self.state.suffix();
            let line = Line::from(vec![
                Span::styled(
                    format!("{}: ", verb.icon_label()),
                    Style::default().fg(verb_color),
                ),
                Span::styled(
                    ExecBox::truncate(&self.command, 30),
                    Style::default().fg(compat::SLATE_200),
                ),
                Span::styled(format!("  {} ", status), Style::default().fg(verb_color)),
                Span::styled(suffix.into_owned(), dim_style),
                Span::styled(format!(" │ {} lines", self.stdout_line_count), dim_style),
            ]);
            return vec![ListItem::new(line)];
        }

        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let content_style = Style::default().fg(compat::SLATE_200);
        let stderr_style = Style::default().fg(compat::AMBER_400); // Amber

        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        let mut items: Vec<ListItem<'static>> = Vec::new();

        // Header line: ╭─ 📟 EXEC ─────────── ✅ 0.3s ───╮
        let header = Line::from(vec![
            Span::styled("╭─ ", border_style),
            Span::styled(verb.icon_label(), border_style),
            Span::styled(" ─── ", border_style),
            Span::styled(format!("{} {}", status_icon, status_suffix), border_style),
            Span::styled(" ─╮", border_style),
        ]);
        items.push(ListItem::new(header));

        // Command line: │ $ command
        let cmd_display = ExecBox::truncate(&self.command, 60);
        let command_line = Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled("$ ", dim_style),
            Span::styled(cmd_display, content_style),
        ]);
        items.push(ListItem::new(command_line));

        // STDOUT section (if not empty)
        if !self.stdout.is_empty() {
            // Separator
            items.push(ListItem::new(Line::from(vec![
                Span::styled("├─ ", border_style),
                Span::styled("stdout", dim_style),
                Span::styled(" ─", border_style),
            ])));

            // STDOUT content lines (limit to first 5 lines in compact mode)
            let max_lines = match self.render_mode {
                RenderMode::Compact => 3,
                RenderMode::Expanded => 10,
                RenderMode::Full => 50,
            };
            for line in self.stdout.lines().take(max_lines) {
                let truncated = ExecBox::truncate(line, 70);
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│   ", border_style),
                    Span::styled(truncated, content_style),
                ])));
            }

            // Overflow indicator
            let line_count = self.stdout_line_count;
            if line_count > max_lines {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│   ", border_style),
                    Span::styled(
                        format!("... +{} more lines", line_count - max_lines),
                        dim_style,
                    ),
                ])));
            }
        }

        // STDERR section (if not empty)
        if !self.stderr.is_empty() {
            // Separator
            items.push(ListItem::new(Line::from(vec![
                Span::styled("├─ ", border_style),
                Span::styled("stderr", stderr_style),
                Span::styled(" ─", border_style),
            ])));

            // STDERR content lines with amber styling
            let max_lines = match self.render_mode {
                RenderMode::Compact => 3,
                RenderMode::Expanded => 10,
                RenderMode::Full => 50,
            };
            for line in self.stderr.lines().take(max_lines) {
                let truncated = ExecBox::truncate(line, 70);
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│   ", border_style),
                    Span::styled(truncated, stderr_style),
                ])));
            }

            // Overflow indicator
            let line_count = self.stderr_line_count;
            if line_count > max_lines {
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│   ", border_style),
                    Span::styled(
                        format!("... +{} more lines", line_count - max_lines),
                        dim_style,
                    ),
                ])));
            }
        }

        // Footer: exit code, pid (if Some), cwd (if Some)
        let exit_style = Style::default().fg(match self.exit_code {
            Some(code) => exit::code_color(code),
            None => crate::tokens::compat::SLATE_500, // Unknown exit code — dim, not green
        });
        let exit_display = self.exit_display();

        let mut footer_spans = vec![
            Span::styled("├─ ", border_style),
            Span::styled(exit_display, exit_style),
        ];

        if let Some(pid) = self.pid {
            footer_spans.push(Span::styled(" │ ", dim_style));
            footer_spans.push(Span::styled(format!("pid: {}", pid), dim_style));
        }

        if let Some(ref cwd) = self.cwd {
            let path = std::path::Path::new(cwd);
            let cwd_display = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(cwd)
                .to_string();
            footer_spans.push(Span::styled(" │ ", dim_style));
            footer_spans.push(Span::styled(cwd_display, dim_style));
        }

        let footer = Line::from(footer_spans);
        items.push(ListItem::new(footer));

        // Bottom border
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "╰────────────────────────────────────────────────────────────────╯",
            border_style,
        )])));

        items
    }
}

impl Widget for ExecBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}

impl Widget for &ExecBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 4 {
            return;
        }

        let verb = VerbColor::Exec;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(compat::SLATE_500);
        let content_style = Style::default().fg(compat::SLATE_200);
        let stderr_style = Style::default().fg(compat::AMBER_400); // Amber

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─ 📟 EXEC ──────────────────────────── ✅ 0.3s ───╮
        let title_prefix = format!("╭─ {} ", verb.icon_label());
        let title_suffix = format!(" {} {} ─╮", status_icon, status_suffix);
        let dash_count = inner_width
            .saturating_sub(display_width(&title_prefix))
            .saturating_sub(display_width(&title_suffix));
        let title = format!("{}{}{}", title_prefix, "─".repeat(dash_count), title_suffix);
        buf.set_string(area.x, area.y, &title, border_style);

        let mut y = area.y + 1;

        // Command line
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let cmd_display = ExecBox::truncate(&self.command, inner_width - 4);
            buf.set_string(area.x + 2, y, format!("$ {}", cmd_display), content_style);
            y += 1;
        }

        // Separator
        if y < area.y + area.height - 1 {
            let sep = format!("├{}┤", "─".repeat(inner_width));
            buf.set_string(area.x, y, &sep, border_style);
            y += 1;
        }

        // STDOUT section
        if !self.stdout.is_empty() && y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            buf.set_string(area.x + 2, y, "STDOUT", dim_style);
            y += 1;

            // Stdout content (iterate directly, no Vec alloc)
            let max_stdout_lines = if self.expanded_stdout { 10 } else { 3 };

            for line in self.stdout.lines().take(max_stdout_lines) {
                if y >= area.y + area.height - 2 {
                    break;
                }
                buf.set_string(area.x, y, "│", border_style);
                buf.set_string(area.x + area.width - 1, y, "│", border_style);

                let line_display = ExecBox::truncate(line, inner_width - 4);
                buf.set_string(area.x + 2, y, format!("┊ {}", line_display), content_style);
                y += 1;
            }
        }

        // STDERR section
        if !self.stderr.is_empty() && y < area.y + area.height - 2 {
            // Separator
            if y < area.y + area.height - 1 {
                let sep = format!("├{}┤", "─".repeat(inner_width));
                buf.set_string(area.x, y, &sep, border_style);
                y += 1;
            }

            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            buf.set_string(area.x + 2, y, "STDERR ⚠️", stderr_style);
            y += 1;

            // Stderr content (iterate directly, no Vec alloc)
            let max_stderr_lines = if self.expanded_stderr { 5 } else { 2 };

            for line in self.stderr.lines().take(max_stderr_lines) {
                if y >= area.y + area.height - 2 {
                    break;
                }
                buf.set_string(area.x, y, "│", border_style);
                buf.set_string(area.x + area.width - 1, y, "│", border_style);

                let line_display = ExecBox::truncate(line, inner_width - 4);
                buf.set_string(area.x + 2, y, format!("┊ {}", line_display), stderr_style);
                y += 1;
            }
        }

        // Fill remaining lines
        while y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            y += 1;
        }

        // Footer with metrics
        let exit_color = self
            .exit_code
            .map(exit::code_color)
            .unwrap_or(compat::SLATE_500);
        let exit_style = Style::default().fg(exit_color);

        let pid_str = self
            .pid
            .map(|p| format!(" │ pid: {}", p))
            .unwrap_or_default();
        let cwd_str = self
            .cwd
            .as_ref()
            .map(|c| {
                // Use display width for terminal column calculation
                let w = display_width(c);
                let display = if w > 20 {
                    // Take last ~17 display-width columns
                    let mut chars: Vec<char> = c.chars().collect();
                    let mut result = String::new();
                    let mut rw = 0;
                    while let Some(ch) = chars.pop() {
                        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                        if rw + cw > 17 {
                            break;
                        }
                        result.insert(0, ch);
                        rw += cw;
                    }
                    format!("...{}", result)
                } else {
                    c.clone()
                };
                format!(" │ cwd: {}", display)
            })
            .unwrap_or_default();

        let footer = format!("│ {}{}{}│", self.exit_display(), pid_str, cwd_str);
        let footer_truncated = ExecBox::truncate(&footer, inner_width + 2);
        buf.set_string(
            area.x,
            area.y + area.height - 2,
            &footer_truncated,
            exit_style,
        );

        // Bottom border
        let bottom = format!("╰{}╯", "─".repeat(inner_width));
        buf.set_string(area.x, area.y + area.height - 1, &bottom, border_style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_box_new() {
        let box_ = ExecBox::new("ls -la");
        assert_eq!(box_.command, "ls -la");
        assert!(box_.stdout.is_empty());
        assert!(box_.stderr.is_empty());
        assert!(box_.exit_code.is_none());
    }

    #[test]
    fn test_exec_box_with_output() {
        let box_ = ExecBox::new("echo hello")
            .with_stdout("hello\n")
            .with_exit_code(0);

        assert_eq!(box_.stdout, "hello\n");
        assert_eq!(box_.exit_code, Some(0));
    }

    #[test]
    fn test_exec_box_with_error() {
        let box_ = ExecBox::new("false")
            .with_stderr("command failed")
            .with_exit_code(1);

        assert_eq!(box_.stderr, "command failed");
        assert_eq!(box_.exit_code, Some(1));
    }

    #[test]
    fn test_append_streams() {
        let mut box_ = ExecBox::new("cmd");
        box_.append_stdout("line1\n");
        box_.append_stdout("line2\n");
        box_.append_stderr("error\n");

        assert_eq!(box_.stdout, "line1\nline2\n");
        assert_eq!(box_.stderr, "error\n");
    }

    #[test]
    fn test_toggle_sections() {
        let mut box_ = ExecBox::new("cmd");
        assert!(!box_.expanded_stdout);
        assert!(!box_.expanded_stderr);

        box_.toggle_stdout();
        assert!(box_.expanded_stdout);

        box_.toggle_stderr();
        assert!(box_.expanded_stderr);
    }

    #[test]
    fn test_exit_display() {
        let success = ExecBox::new("cmd").with_exit_code(0);
        assert!(success.exit_display().contains("0"));

        let failure = ExecBox::new("cmd").with_exit_code(1);
        assert!(failure.exit_display().contains("1"));

        let running = ExecBox::new("cmd");
        assert!(running.exit_display().contains("?"));
    }

    #[test]
    fn test_with_pid_and_cwd() {
        let box_ = ExecBox::new("cmd")
            .with_pid(12345)
            .with_cwd("/home/user/project");

        assert_eq!(box_.pid, Some(12345));
        assert_eq!(box_.cwd, Some("/home/user/project".to_string()));
    }

    #[test]
    fn test_required_height() {
        let minimal = ExecBox::new("cmd");
        assert!(minimal.required_height() >= 4);

        let with_output = ExecBox::new("cmd").with_stdout("output");
        assert!(with_output.required_height() > minimal.required_height());
    }

    #[test]
    fn test_exec_box_with_pulse() {
        let box_ = ExecBox::new("ls -la").with_pulse_intensity(0.7);
        assert!((box_.pulse_intensity - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_exec_box_pulse_default_zero() {
        let box_ = ExecBox::new("ls -la");
        assert!((box_.pulse_intensity - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_exec_box_pulse_clamped() {
        let box_high = ExecBox::new("cmd").with_pulse_intensity(1.5);
        assert!((box_high.pulse_intensity - 1.0).abs() < 0.001);

        let box_low = ExecBox::new("cmd").with_pulse_intensity(-0.5);
        assert!((box_low.pulse_intensity - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_with_stdout_populates_line_count() {
        let box_ = ExecBox::new("test").with_stdout("line 1\nline 2\nline 3\n");
        assert_eq!(
            box_.stdout_line_count, 3,
            "with_stdout must populate stdout_line_count"
        );
    }

    #[test]
    fn test_with_stderr_populates_line_count() {
        let box_ = ExecBox::new("test").with_stderr("err 1\nerr 2\n");
        assert_eq!(
            box_.stderr_line_count, 2,
            "with_stderr must populate stderr_line_count"
        );
    }
}
