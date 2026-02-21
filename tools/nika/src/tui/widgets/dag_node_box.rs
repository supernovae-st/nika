//! DAG Node Box Widget
//!
//! Renders individual task nodes in the DAG visualization.
//! Supports minimal and expanded display modes with status-specific styling.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
};

use crate::tui::theme::{TaskStatus, VerbColor};

// ═══════════════════════════════════════════════════════════════════════════
// ANIMATION CONSTANTS
// ═══════════════════════════════════════════════════════════════════════════

/// Spinner animation frames for running tasks
const SPINNER_FRAMES: &[&str] = &["◐", "◓", "◑", "◒"];

/// Success celebration frames
const SUCCESS_FRAMES: &[&str] = &["✓", "✔", "✓", "✔"];

/// Progress bar characters
const PROGRESS_EMPTY: char = '░';
const PROGRESS_FILLED: char = '▓';
const PROGRESS_PARTIAL: char = '▒';

// ═══════════════════════════════════════════════════════════════════════════
// NODE BOX MODE
// ═══════════════════════════════════════════════════════════════════════════

/// Display mode for node boxes
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NodeBoxMode {
    /// Compact display: icon + id + estimate + badge (3 lines)
    #[default]
    Minimal,
    /// Full display: adds prompt preview and model (5+ lines)
    Expanded,
}

// ═══════════════════════════════════════════════════════════════════════════
// NODE BOX DATA
// ═══════════════════════════════════════════════════════════════════════════

/// Node data for rendering
#[derive(Debug, Clone)]
pub struct NodeBoxData {
    /// Task ID
    pub id: String,
    /// Verb type
    pub verb: VerbColor,
    /// Current status
    pub status: TaskStatus,
    /// Estimated duration
    pub estimate: String,
    /// Prompt preview (for expanded mode)
    pub prompt_preview: Option<String>,
    /// Model name (for expanded mode)
    pub model: Option<String>,
    /// For_each items count
    pub for_each_count: Option<usize>,
    /// For_each item names (for mini-nodes)
    pub for_each_items: Vec<String>,
}

impl NodeBoxData {
    /// Create new node data with required fields
    pub fn new(id: impl Into<String>, verb: VerbColor) -> Self {
        Self {
            id: id.into(),
            verb,
            status: TaskStatus::Pending,
            estimate: String::new(),
            prompt_preview: None,
            model: None,
            for_each_count: None,
            for_each_items: Vec::new(),
        }
    }

    /// Set the task status
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the estimated duration
    pub fn with_estimate(mut self, estimate: impl Into<String>) -> Self {
        self.estimate = estimate.into();
        self
    }

    /// Set the prompt preview text
    pub fn with_prompt_preview(mut self, preview: impl Into<String>) -> Self {
        self.prompt_preview = Some(preview.into());
        self
    }

    /// Set the model name
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set for_each count
    pub fn with_for_each_count(mut self, count: usize) -> Self {
        self.for_each_count = Some(count);
        self
    }

    /// Set for_each item names
    pub fn with_for_each_items(mut self, items: Vec<String>) -> Self {
        self.for_each_items = items;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BORDER STYLE
// ═══════════════════════════════════════════════════════════════════════════

/// Border style for node boxes
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BorderStyle {
    /// Sharp corners (default)
    #[default]
    Sharp,
    /// Rounded corners using ╭╮╰╯
    Rounded,
}

// ═══════════════════════════════════════════════════════════════════════════
// BORDER CHARACTERS
// ═══════════════════════════════════════════════════════════════════════════

/// Border character set based on status
#[derive(Debug, Clone, Copy)]
struct BorderChars {
    /// Top-left corner
    tl: char,
    /// Top-right corner
    tr: char,
    /// Bottom-left corner
    bl: char,
    /// Bottom-right corner
    br: char,
    /// Horizontal line
    h: char,
    /// Vertical line
    v: char,
}

impl BorderChars {
    /// Get border characters for a task status with optional rounded corners
    fn for_status(status: TaskStatus, style: BorderStyle) -> Self {
        match (status, style) {
            // Pending: dashed borders (light) - rounded
            (TaskStatus::Pending, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '┄',
                v: '┆',
            },
            // Pending: dashed borders (light) - sharp
            (TaskStatus::Pending, BorderStyle::Sharp) => Self {
                tl: '┌',
                tr: '┐',
                bl: '└',
                br: '┘',
                h: '┄',
                v: '┆',
            },
            // Running: bold/heavy borders - rounded
            (TaskStatus::Running, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '━',
                v: '┃',
            },
            // Running: bold/heavy borders - sharp
            (TaskStatus::Running, BorderStyle::Sharp) => Self {
                tl: '┏',
                tr: '┓',
                bl: '┗',
                br: '┛',
                h: '━',
                v: '┃',
            },
            // Success: double line borders - rounded
            (TaskStatus::Success, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '═',
                v: '║',
            },
            // Success: double line borders - sharp
            (TaskStatus::Success, BorderStyle::Sharp) => Self {
                tl: '╔',
                tr: '╗',
                bl: '╚',
                br: '╝',
                h: '═',
                v: '║',
            },
            // Failed: double line borders - rounded
            (TaskStatus::Failed, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '═',
                v: '║',
            },
            // Failed: double line borders - sharp
            (TaskStatus::Failed, BorderStyle::Sharp) => Self {
                tl: '╔',
                tr: '╗',
                bl: '╚',
                br: '╝',
                h: '═',
                v: '║',
            },
            // Paused: dotted borders - rounded
            (TaskStatus::Paused, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '┈',
                v: '┊',
            },
            // Paused: dotted borders - sharp
            (TaskStatus::Paused, BorderStyle::Sharp) => Self {
                tl: '┌',
                tr: '┐',
                bl: '└',
                br: '┘',
                h: '┈',
                v: '┊',
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NODE BOX WIDGET
// ═══════════════════════════════════════════════════════════════════════════

/// Node box widget for DAG visualization
pub struct NodeBox<'a> {
    /// Node data
    data: &'a NodeBoxData,
    /// Display mode
    mode: NodeBoxMode,
    /// Whether this node is focused/selected
    focused: bool,
    /// Animation frame (0-255) for spinners and effects
    frame: u8,
    /// Border style (sharp or rounded)
    border_style: BorderStyle,
    /// Progress percentage (0-100) for running tasks
    progress: Option<u8>,
}

impl<'a> NodeBox<'a> {
    /// Create a new node box widget
    pub fn new(data: &'a NodeBoxData) -> Self {
        Self {
            data,
            mode: NodeBoxMode::default(),
            focused: false,
            frame: 0,
            border_style: BorderStyle::Rounded, // Default to rounded for modern look
            progress: None,
        }
    }

    /// Set the display mode
    pub fn mode(mut self, mode: NodeBoxMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set whether this node is focused
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Set the animation frame (0-255)
    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Set the border style
    pub fn with_border_style(mut self, style: BorderStyle) -> Self {
        self.border_style = style;
        self
    }

    /// Set progress percentage for running tasks (0-100)
    pub fn progress(mut self, progress: Option<u8>) -> Self {
        self.progress = progress.map(|p| p.min(100));
        self
    }

    /// Calculate required width for this node
    pub fn required_width(&self) -> u16 {
        // Minimum width calculation:
        // icon (2) + space (1) + id + space (1) + estimate + space (1) + badge (1)
        // Plus 2 for borders
        let icon_width = 2; // emoji width
        let badge_width = 1;
        let spacing = 3; // spaces between elements
        let borders = 2;

        let id_width = self.data.id.len();
        let estimate_width = self.data.estimate.len();

        let content_width = icon_width + id_width + estimate_width + badge_width + spacing;

        // Add for_each indicator if present
        let for_each_width = self.data.for_each_count.map_or(0, |count| {
            // Format: " x3" or " x10"
            format!(" x{}", count).len()
        });

        (content_width + for_each_width + borders).max(12) as u16
    }

    /// Calculate required height for this node
    pub fn required_height(&self) -> u16 {
        match self.mode {
            NodeBoxMode::Minimal => 3, // top border + content + bottom border
            NodeBoxMode::Expanded => {
                let mut height = 3; // base
                if self.data.prompt_preview.is_some() {
                    height += 1;
                }
                if self.data.model.is_some() {
                    height += 1;
                }
                height
            }
        }
    }

    /// Get the status badge character (animated for running/success)
    fn status_badge(&self) -> &'static str {
        match self.data.status {
            TaskStatus::Pending => "○",
            TaskStatus::Running => {
                // Animated spinner
                let idx = (self.frame as usize / 4) % SPINNER_FRAMES.len();
                SPINNER_FRAMES[idx]
            }
            TaskStatus::Success => {
                // Subtle celebration animation
                let idx = (self.frame as usize / 8) % SUCCESS_FRAMES.len();
                SUCCESS_FRAMES[idx]
            }
            TaskStatus::Failed => "✗",
            TaskStatus::Paused => "◐",
        }
    }

    /// Render a mini progress bar for running tasks
    fn render_progress_bar(&self, buf: &mut Buffer, x: u16, y: u16, width: u16) {
        if let Some(progress) = self.progress {
            let filled = ((progress as u16) * width) / 100;
            let style = Style::default().fg(ratatui::style::Color::Rgb(34, 197, 94)); // green

            for i in 0..width {
                let ch = if i < filled {
                    PROGRESS_FILLED
                } else if i == filled && progress > 0 {
                    PROGRESS_PARTIAL
                } else {
                    PROGRESS_EMPTY
                };
                buf.set_string(x + i, y, ch.to_string(), style);
            }
        }
    }

    /// Get border render style based on status
    fn border_render_style(&self) -> Style {
        let color = match self.data.status {
            TaskStatus::Pending => self.data.verb.muted(),
            TaskStatus::Running => self.data.verb.rgb(),
            TaskStatus::Success => self.data.verb.rgb(),
            TaskStatus::Failed => ratatui::style::Color::Rgb(239, 68, 68), // red
            TaskStatus::Paused => self.data.verb.muted(),
        };

        let mut style = Style::default().fg(color);

        if self.focused {
            style = style.add_modifier(Modifier::BOLD);
        }

        if self.data.status == TaskStatus::Running {
            style = style.add_modifier(Modifier::BOLD);
        }

        style
    }

    /// Get content style
    fn content_style(&self) -> Style {
        let color = match self.data.status {
            TaskStatus::Pending => ratatui::style::Color::Rgb(156, 163, 175), // gray-400
            TaskStatus::Running => ratatui::style::Color::Rgb(243, 244, 246), // gray-100
            TaskStatus::Success => ratatui::style::Color::Rgb(243, 244, 246), // gray-100
            TaskStatus::Failed => ratatui::style::Color::Rgb(239, 68, 68),    // red
            TaskStatus::Paused => ratatui::style::Color::Rgb(156, 163, 175),  // gray-400
        };

        Style::default().fg(color)
    }

    /// Get badge style
    fn badge_style(&self) -> Style {
        let color = match self.data.status {
            TaskStatus::Pending => ratatui::style::Color::Rgb(107, 114, 128), // gray-500
            TaskStatus::Running => ratatui::style::Color::Rgb(245, 158, 11),  // amber
            TaskStatus::Success => ratatui::style::Color::Rgb(34, 197, 94),   // green
            TaskStatus::Failed => ratatui::style::Color::Rgb(239, 68, 68),    // red
            TaskStatus::Paused => ratatui::style::Color::Rgb(6, 182, 212),    // cyan
        };

        Style::default().fg(color)
    }
}

impl Widget for NodeBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 5 {
            return;
        }

        let border_chars = BorderChars::for_status(self.data.status, self.border_style);
        let border_render_style = self.border_render_style();
        let content_style = self.content_style();

        // Draw top border
        buf.set_string(
            area.x,
            area.y,
            border_chars.tl.to_string(),
            border_render_style,
        );
        for x in (area.x + 1)..(area.x + area.width - 1) {
            buf.set_string(x, area.y, border_chars.h.to_string(), border_render_style);
        }
        buf.set_string(
            area.x + area.width - 1,
            area.y,
            border_chars.tr.to_string(),
            border_render_style,
        );

        // Draw content line (y + 1)
        let content_y = area.y + 1;
        buf.set_string(
            area.x,
            content_y,
            border_chars.v.to_string(),
            border_render_style,
        );

        // Clear content area
        for x in (area.x + 1)..(area.x + area.width - 1) {
            buf.set_string(x, content_y, " ", content_style);
        }

        // Build content: icon + id + estimate + badge
        let icon = self.data.verb.icon();
        let badge = self.status_badge();

        // Position content
        let mut x = area.x + 1;
        let max_x = area.x + area.width - 2;

        // Icon
        if x + 2 <= max_x {
            buf.set_string(x, content_y, icon, content_style);
            x += 2; // emoji width
        }

        // Space
        if x < max_x {
            x += 1;
        }

        // ID (truncate if needed)
        let available_for_id = (max_x - x).saturating_sub(self.data.estimate.len() as u16 + 3);
        let id_display = if self.data.id.len() as u16 > available_for_id && available_for_id > 3 {
            format!(
                "{}...",
                &self.data.id[..(available_for_id as usize - 3).max(1)]
            )
        } else {
            self.data.id.clone()
        };

        if x + id_display.len() as u16 <= max_x {
            buf.set_string(x, content_y, &id_display, content_style);
            x += id_display.len() as u16;
        }

        // For_each indicator
        if let Some(count) = self.data.for_each_count {
            let indicator = format!(" x{}", count);
            if x + indicator.len() as u16 <= max_x {
                buf.set_string(
                    x,
                    content_y,
                    &indicator,
                    Style::default().fg(ratatui::style::Color::Rgb(139, 92, 246)), // violet
                );
                x += indicator.len() as u16;
            }
        }

        // Space before estimate
        if x < max_x && !self.data.estimate.is_empty() {
            x += 1;
        }

        // Estimate
        if !self.data.estimate.is_empty() && x + self.data.estimate.len() as u16 + 2 <= max_x {
            buf.set_string(
                x,
                content_y,
                &self.data.estimate,
                Style::default().fg(ratatui::style::Color::Rgb(107, 114, 128)), // gray-500
            );
            x += self.data.estimate.len() as u16;
        }

        // Badge at the end
        let badge_x = area.x + area.width - 2;
        if badge_x > x {
            buf.set_string(badge_x, content_y, badge, self.badge_style());
        }

        buf.set_string(
            area.x + area.width - 1,
            content_y,
            border_chars.v.to_string(),
            border_render_style,
        );

        // Expanded mode: additional lines
        if self.mode == NodeBoxMode::Expanded && area.height >= 4 {
            let mut extra_y = content_y + 1;

            // Model line
            if let Some(model) = &self.data.model {
                if extra_y < area.y + area.height - 1 {
                    buf.set_string(
                        area.x,
                        extra_y,
                        border_chars.v.to_string(),
                        border_render_style,
                    );
                    for x in (area.x + 1)..(area.x + area.width - 1) {
                        buf.set_string(x, extra_y, " ", content_style);
                    }
                    let model_text = format!(" {}", model);
                    let truncated = if model_text.len() as u16 > area.width - 3 {
                        format!("{}...", &model_text[..(area.width as usize - 6).max(3)])
                    } else {
                        model_text
                    };
                    buf.set_string(
                        area.x + 1,
                        extra_y,
                        &truncated,
                        Style::default().fg(ratatui::style::Color::Rgb(156, 163, 175)),
                    );
                    buf.set_string(
                        area.x + area.width - 1,
                        extra_y,
                        border_chars.v.to_string(),
                        border_render_style,
                    );
                    extra_y += 1;
                }
            }

            // Prompt preview line
            if let Some(preview) = &self.data.prompt_preview {
                if extra_y < area.y + area.height - 1 {
                    buf.set_string(
                        area.x,
                        extra_y,
                        border_chars.v.to_string(),
                        border_render_style,
                    );
                    for x in (area.x + 1)..(area.x + area.width - 1) {
                        buf.set_string(x, extra_y, " ", content_style);
                    }
                    let preview_text = format!(" \"{}\"", preview);
                    let truncated = if preview_text.len() as u16 > area.width - 3 {
                        format!("{}...\"", &preview_text[..(area.width as usize - 7).max(3)])
                    } else {
                        preview_text
                    };
                    buf.set_string(
                        area.x + 1,
                        extra_y,
                        &truncated,
                        Style::default()
                            .fg(ratatui::style::Color::Rgb(156, 163, 175))
                            .add_modifier(Modifier::ITALIC),
                    );
                    buf.set_string(
                        area.x + area.width - 1,
                        extra_y,
                        border_chars.v.to_string(),
                        border_render_style,
                    );
                }
            }
        }

        // Draw bottom border
        let bottom_y = area.y + area.height - 1;
        buf.set_string(
            area.x,
            bottom_y,
            border_chars.bl.to_string(),
            border_render_style,
        );
        for x in (area.x + 1)..(area.x + area.width - 1) {
            buf.set_string(x, bottom_y, border_chars.h.to_string(), border_render_style);
        }
        buf.set_string(
            area.x + area.width - 1,
            bottom_y,
            border_chars.br.to_string(),
            border_render_style,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_box_creation() {
        let data = NodeBoxData::new("generate", VerbColor::Infer)
            .with_status(TaskStatus::Running)
            .with_estimate("~2s");

        assert_eq!(data.id, "generate");
        assert_eq!(data.verb, VerbColor::Infer);
        assert_eq!(data.status, TaskStatus::Running);
        assert_eq!(data.estimate, "~2s");
    }

    #[test]
    fn test_node_box_data_builder_methods() {
        let data = NodeBoxData::new("task1", VerbColor::Exec)
            .with_status(TaskStatus::Success)
            .with_estimate("1.5s")
            .with_prompt_preview("Generate landing page...")
            .with_model("claude-sonnet-4")
            .with_for_each_count(3)
            .with_for_each_items(vec!["a".into(), "b".into(), "c".into()]);

        assert_eq!(data.id, "task1");
        assert_eq!(data.verb, VerbColor::Exec);
        assert_eq!(data.status, TaskStatus::Success);
        assert_eq!(data.estimate, "1.5s");
        assert_eq!(
            data.prompt_preview,
            Some("Generate landing page...".to_string())
        );
        assert_eq!(data.model, Some("claude-sonnet-4".to_string()));
        assert_eq!(data.for_each_count, Some(3));
        assert_eq!(data.for_each_items, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_required_dimensions() {
        let data = NodeBoxData::new("task", VerbColor::Infer).with_estimate("~1s");

        let widget = NodeBox::new(&data);

        // Minimal mode: 3 lines
        assert_eq!(widget.required_height(), 3);

        // Width: icon(2) + space(1) + id(4) + space(1) + estimate(3) + space(1) + badge(1) + borders(2) = 15
        // But minimum is 12
        let width = widget.required_width();
        assert!(width >= 12, "Width {} should be >= 12", width);
    }

    #[test]
    fn test_required_height_expanded() {
        let data = NodeBoxData::new("task", VerbColor::Infer)
            .with_prompt_preview("Test prompt")
            .with_model("claude-sonnet");

        let widget = NodeBox::new(&data).mode(NodeBoxMode::Expanded);

        // Expanded: 3 base + 1 prompt + 1 model = 5
        assert_eq!(widget.required_height(), 5);
    }

    #[test]
    fn test_border_chars_by_status_sharp() {
        // Pending: dashed (sharp)
        let pending = BorderChars::for_status(TaskStatus::Pending, BorderStyle::Sharp);
        assert_eq!(pending.h, '┄');
        assert_eq!(pending.v, '┆');
        assert_eq!(pending.tl, '┌');

        // Running: bold (sharp)
        let running = BorderChars::for_status(TaskStatus::Running, BorderStyle::Sharp);
        assert_eq!(running.h, '━');
        assert_eq!(running.v, '┃');
        assert_eq!(running.tl, '┏');

        // Success: double (sharp)
        let success = BorderChars::for_status(TaskStatus::Success, BorderStyle::Sharp);
        assert_eq!(success.h, '═');
        assert_eq!(success.v, '║');
        assert_eq!(success.tl, '╔');

        // Failed: double (sharp, same chars, different color)
        let failed = BorderChars::for_status(TaskStatus::Failed, BorderStyle::Sharp);
        assert_eq!(failed.h, '═');
        assert_eq!(failed.v, '║');

        // Paused: dotted (sharp)
        let paused = BorderChars::for_status(TaskStatus::Paused, BorderStyle::Sharp);
        assert_eq!(paused.h, '┈');
        assert_eq!(paused.v, '┊');
    }

    #[test]
    fn test_border_chars_by_status_rounded() {
        // Pending: dashed (rounded)
        let pending = BorderChars::for_status(TaskStatus::Pending, BorderStyle::Rounded);
        assert_eq!(pending.h, '┄');
        assert_eq!(pending.v, '┆');
        assert_eq!(pending.tl, '╭'); // Rounded corner

        // Running: bold (rounded)
        let running = BorderChars::for_status(TaskStatus::Running, BorderStyle::Rounded);
        assert_eq!(running.h, '━');
        assert_eq!(running.v, '┃');
        assert_eq!(running.tl, '╭'); // Rounded corner

        // Success: double (rounded)
        let success = BorderChars::for_status(TaskStatus::Success, BorderStyle::Rounded);
        assert_eq!(success.h, '═');
        assert_eq!(success.v, '║');
        assert_eq!(success.tl, '╭'); // Rounded corner
    }

    #[test]
    fn test_minimal_vs_expanded_height() {
        let data = NodeBoxData::new("test", VerbColor::Agent)
            .with_prompt_preview("Some prompt")
            .with_model("gpt-4");

        // Minimal mode
        let minimal = NodeBox::new(&data).mode(NodeBoxMode::Minimal);
        assert_eq!(minimal.required_height(), 3);

        // Expanded mode
        let expanded = NodeBox::new(&data).mode(NodeBoxMode::Expanded);
        assert_eq!(expanded.required_height(), 5);
    }

    #[test]
    fn test_status_badges() {
        let data = NodeBoxData::new("t", VerbColor::Infer);
        let widget = NodeBox::new(&data);
        assert_eq!(widget.status_badge(), "○");

        // Running badge is animated - check it's one of the spinner frames
        let data = NodeBoxData::new("t", VerbColor::Infer).with_status(TaskStatus::Running);
        let widget = NodeBox::new(&data);
        assert!(
            SPINNER_FRAMES.contains(&widget.status_badge()),
            "Running badge should be a spinner frame"
        );

        // Success badge is animated - check it's one of the success frames
        let data = NodeBoxData::new("t", VerbColor::Infer).with_status(TaskStatus::Success);
        let widget = NodeBox::new(&data);
        assert!(
            SUCCESS_FRAMES.contains(&widget.status_badge()),
            "Success badge should be a success frame"
        );

        let data = NodeBoxData::new("t", VerbColor::Infer).with_status(TaskStatus::Failed);
        let widget = NodeBox::new(&data);
        assert_eq!(widget.status_badge(), "✗");

        let data = NodeBoxData::new("t", VerbColor::Infer).with_status(TaskStatus::Paused);
        let widget = NodeBox::new(&data);
        assert_eq!(widget.status_badge(), "◐");
    }

    #[test]
    fn test_node_box_mode_default() {
        let mode = NodeBoxMode::default();
        assert_eq!(mode, NodeBoxMode::Minimal);
    }

    #[test]
    fn test_node_box_with_for_each() {
        let data = NodeBoxData::new("parallel", VerbColor::Fetch)
            .with_for_each_count(10)
            .with_for_each_items(vec!["a".into(), "b".into()]);

        let widget = NodeBox::new(&data);

        // Width should include the " x10" indicator
        let width = widget.required_width();
        let data_without = NodeBoxData::new("parallel", VerbColor::Fetch);
        let width_without = NodeBox::new(&data_without).required_width();

        assert!(
            width > width_without,
            "Width with for_each ({}) should be > without ({})",
            width,
            width_without
        );
    }

    #[test]
    fn test_node_box_rendering_does_not_panic() {
        let data = NodeBoxData::new("test_task", VerbColor::Exec)
            .with_status(TaskStatus::Running)
            .with_estimate("~3s")
            .with_prompt_preview("Test prompt preview")
            .with_model("claude-sonnet-4");

        let widget = NodeBox::new(&data)
            .mode(NodeBoxMode::Expanded)
            .focused(true);

        // Create a small buffer and render
        let mut buffer = Buffer::empty(Rect::new(0, 0, 30, 6));
        widget.render(Rect::new(0, 0, 30, 6), &mut buffer);

        // Should have rendered without panic
        // Check that top-left corner is set (default is rounded)
        let cell = buffer.cell((0, 0)).unwrap();
        assert_eq!(cell.symbol(), "╭"); // Running status = rounded corner (default)
    }

    #[test]
    fn test_node_box_rendering_sharp_borders() {
        let data = NodeBoxData::new("test_task", VerbColor::Exec).with_status(TaskStatus::Running);

        let widget = NodeBox::new(&data).with_border_style(BorderStyle::Sharp);

        // Create a small buffer and render
        let mut buffer = Buffer::empty(Rect::new(0, 0, 30, 6));
        widget.render(Rect::new(0, 0, 30, 6), &mut buffer);

        // Check that top-left corner is sharp
        let cell = buffer.cell((0, 0)).unwrap();
        assert_eq!(cell.symbol(), "┏"); // Running status = sharp bold border
    }

    #[test]
    fn test_node_box_renders_in_minimal_area() {
        let data = NodeBoxData::new("x", VerbColor::Infer);
        let widget = NodeBox::new(&data);

        // Minimum renderable area
        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 3));
        widget.render(Rect::new(0, 0, 10, 3), &mut buffer);

        // Should render without panic
        let cell = buffer.cell((0, 0)).unwrap();
        assert!(!cell.symbol().is_empty());
    }

    #[test]
    fn test_node_box_skips_render_if_too_small() {
        let data = NodeBoxData::new("x", VerbColor::Infer);
        let widget = NodeBox::new(&data);

        // Too small - should skip rendering
        let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 2));
        widget.render(Rect::new(0, 0, 3, 2), &mut buffer);

        // Buffer should be empty (default cells)
        let cell = buffer.cell((0, 0)).unwrap();
        assert_eq!(cell.symbol(), " ");
    }
}
