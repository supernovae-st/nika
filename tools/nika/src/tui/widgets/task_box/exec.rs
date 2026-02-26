//! ExecBox Widget
//!
//! Shell command execution box with stdout/stderr separation.
//! Shows command, output streams, exit code, and timing.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use super::{exit, BoxState, RenderMode, VerbColor};

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
        self.stdout = stdout.into();
        self
    }

    /// Set stderr
    pub fn with_stderr(mut self, stderr: impl Into<String>) -> Self {
        self.stderr = stderr.into();
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

    /// Set pulse intensity for border animation (clamped to 0.0-1.0)
    pub fn with_pulse_intensity(mut self, intensity: f32) -> Self {
        self.pulse_intensity = intensity.clamp(0.0, 1.0);
        self
    }

    /// Set render mode
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Append to stdout (streaming)
    pub fn append_stdout(&mut self, text: &str) {
        self.stdout.push_str(text);
    }

    /// Append to stderr (streaming)
    pub fn append_stderr(&mut self, text: &str) {
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
        // Compact mode is always 1 line
        if self.render_mode == RenderMode::Compact {
            return 1;
        }

        let mut height: u16 = 4; // Header + command + footer + bottom border

        // Stdout lines
        if !self.stdout.is_empty() {
            height += 2; // Header + content
            if self.expanded_stdout {
                height += self.stdout.lines().count().min(10) as u16;
            }
        }

        // Stderr lines
        if !self.stderr.is_empty() {
            height += 2; // Header + content
            if self.expanded_stderr {
                height += self.stderr.lines().count().min(5) as u16;
            }
        }

        height
    }

    /// Truncate string preserving UTF-8
    fn truncate(s: &str, max_len: usize) -> String {
        let char_count = s.chars().count();
        if char_count > max_len && max_len > 3 {
            let truncated: String = s.chars().take(max_len - 3).collect();
            format!("{}...", truncated)
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

    /// Render in compact mode (single line)
    fn render_compact(&self, area: Rect, buf: &mut Buffer) {
        let verb = VerbColor::Exec;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let line_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(Color::Rgb(100, 116, 139));

        let status_icon = self.state.icon();
        let exit_info = self.exit_display();

        // Format: 📟 EXEC: ls -la  ✅ exit: 0
        let prefix = format!("{} ", verb.icon_label());
        let suffix = format!("  {} {}", status_icon, exit_info);
        let available = (area.width as usize)
            .saturating_sub(prefix.chars().count())
            .saturating_sub(suffix.chars().count());
        let cmd = Self::truncate(&self.command, available);

        let line = format!("{}{}{}", prefix, cmd, suffix);
        buf.set_string(area.x, area.y, &line, line_style);

        // Dim the exit info portion
        let exit_x = area.x + (line.chars().count() - suffix.chars().count()) as u16;
        buf.set_string(exit_x, area.y, &suffix, dim_style);
    }
}

impl Widget for ExecBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Compact mode: single line
        if self.render_mode == RenderMode::Compact {
            if area.width < 20 || area.height < 1 {
                return;
            }
            self.render_compact(area, buf);
            return;
        }

        if area.width < 30 || area.height < 4 {
            return;
        }

        let verb = VerbColor::Exec;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(Color::Rgb(100, 116, 139));
        let content_style = Style::default().fg(Color::Rgb(226, 232, 240));
        let stderr_style = Style::default().fg(Color::Rgb(251, 191, 36)); // Amber

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─ 📟 EXEC ──────────────────────────── ✅ 0.3s ───╮
        let title_prefix = format!("╭─ {} ", verb.icon_label());
        let title_suffix = format!(" {} {} ─╮", status_icon, status_suffix);
        let dash_count = inner_width
            .saturating_sub(title_prefix.chars().count())
            .saturating_sub(title_suffix.chars().count());
        let title = format!("{}{}{}", title_prefix, "─".repeat(dash_count), title_suffix);
        buf.set_string(area.x, area.y, &title, border_style);

        let mut y = area.y + 1;

        // Command line
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let cmd_display = Self::truncate(&self.command, inner_width - 4);
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

            // Stdout content
            let stdout_lines: Vec<&str> = if self.expanded_stdout {
                self.stdout.lines().take(10).collect()
            } else {
                self.stdout.lines().take(3).collect()
            };

            for line in stdout_lines {
                if y >= area.y + area.height - 2 {
                    break;
                }
                buf.set_string(area.x, y, "│", border_style);
                buf.set_string(area.x + area.width - 1, y, "│", border_style);

                let line_display = Self::truncate(line, inner_width - 4);
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

            // Stderr content
            let stderr_lines: Vec<&str> = if self.expanded_stderr {
                self.stderr.lines().take(5).collect()
            } else {
                self.stderr.lines().take(2).collect()
            };

            for line in stderr_lines {
                if y >= area.y + area.height - 2 {
                    break;
                }
                buf.set_string(area.x, y, "│", border_style);
                buf.set_string(area.x + area.width - 1, y, "│", border_style);

                let line_display = Self::truncate(line, inner_width - 4);
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
            .unwrap_or(Color::Rgb(100, 116, 139));
        let exit_style = Style::default().fg(exit_color);

        let pid_str = self
            .pid
            .map(|p| format!(" │ pid: {}", p))
            .unwrap_or_default();
        let cwd_str = self
            .cwd
            .as_ref()
            .map(|c| {
                // PERF: Use char count, not byte length (UTF-8 safe)
                let char_count = c.chars().count();
                let display = if char_count > 20 {
                    let truncated: String = c.chars().skip(char_count - 17).collect();
                    format!("...{}", truncated)
                } else {
                    c.clone()
                };
                format!(" │ cwd: {}", display)
            })
            .unwrap_or_default();

        let footer = format!("│ {}{}{}│", self.exit_display(), pid_str, cwd_str);
        let footer_truncated = Self::truncate(&footer, inner_width + 2);
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

    // === RenderMode tests ===

    #[test]
    fn test_exec_box_with_render_mode() {
        let box_ = ExecBox::new("ls -la").with_render_mode(RenderMode::Compact);
        assert_eq!(box_.render_mode, RenderMode::Compact);
    }

    #[test]
    fn test_exec_box_compact_required_height() {
        let compact = ExecBox::new("ls -la").with_render_mode(RenderMode::Compact);
        assert_eq!(compact.required_height(), 1);

        let expanded = ExecBox::new("ls -la").with_render_mode(RenderMode::Expanded);
        assert!(expanded.required_height() >= 4);
    }

    #[test]
    fn test_exec_box_compact_render() {
        // Verify compact mode produces a single-line output
        let box_ = ExecBox::new("ls -la")
            .with_render_mode(RenderMode::Compact)
            .with_exit_code(0);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 1));
        box_.render(Rect::new(0, 0, 60, 1), &mut buf);

        // Check the buffer has content on the first line
        let line: String = (0..60)
            .map(|x| {
                buf.cell((x, 0))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(line.contains("EXEC"));
        assert!(line.contains("ls"));
    }
}
