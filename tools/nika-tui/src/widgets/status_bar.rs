// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unified status bar widget showing contextual keybindings
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │ [Enter] Send  [Up/Down] History     Claude | 1.2k tokens | MCP: 2 connected │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use std::borrow::Cow;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use unicode_width::UnicodeWidthStr;

use crate::icons::provider as icons_provider;
use crate::keybindings::{format_key, keybindings_for_context, KeyCategory, Keybinding};
use crate::mode::InputMode;
use crate::theme::Theme;
use crate::views::TuiView;

/// Key hint for status bar
///
/// Uses `Cow<'static, str>` to avoid memory leaks - static strings don't allocate,
/// while dynamically generated keys (from keybindings) are owned without Box::leak.
#[derive(Debug, Clone)]
pub struct KeyHint {
    pub key: Cow<'static, str>,
    pub action: Cow<'static, str>,
}

impl KeyHint {
    /// Create a new key hint from static strings (zero allocation)
    pub const fn new(key: &'static str, action: &'static str) -> Self {
        Self {
            key: Cow::Borrowed(key),
            action: Cow::Borrowed(action),
        }
    }

    /// Create a new key hint with an owned key string (for dynamic keys)
    pub fn with_owned_key(key: String, action: &'static str) -> Self {
        Self {
            key: Cow::Owned(key),
            action: Cow::Borrowed(action),
        }
    }
}

/// LLM Provider indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Provider {
    #[default]
    None,
    Claude,
    OpenAI,
    Mistral,
    Native,
    Groq,
    DeepSeek,
    Mock,
}

impl Provider {
    /// Get provider icon (delegates to canonical icons::provider)
    pub fn icon(&self) -> &'static str {
        match self {
            Self::None => "  ",
            Self::Claude => icons_provider::CLAUDE,
            Self::OpenAI => icons_provider::OPENAI,
            Self::Mistral => icons_provider::MISTRAL,
            Self::Native => icons_provider::NATIVE,
            Self::Groq => icons_provider::GROQ,
            Self::DeepSeek => icons_provider::DEEPSEEK,
            Self::Mock => icons_provider::MOCK,
        }
    }

    /// Get provider display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "---",
            Self::Claude => "Claude",
            Self::OpenAI => "OpenAI",
            Self::Mistral => "Mistral",
            Self::Native => "Native",
            Self::Groq => "Groq",
            Self::DeepSeek => "DeepSeek",
            Self::Mock => "Mock",
        }
    }

    /// Detect provider from model name (called once when model changes, not every frame).
    ///
    /// Converts the model name to lowercase once and checks provider patterns.
    pub fn from_model_name(model: &str) -> Self {
        let model_lower = model.to_lowercase();
        if model_lower.contains("claude") {
            Self::Claude
        } else if model_lower.contains("gpt") || model_lower.contains("openai") {
            Self::OpenAI
        } else if model_lower.contains("mistral") || model_lower.contains("mixtral") {
            Self::Mistral
        } else if model_lower.contains("gguf")
            || model_lower.contains("native")
            || model_lower.contains("local")
        {
            Self::Native
        } else if model_lower.contains("groq") {
            Self::Groq
        } else if model_lower.contains("deepseek") {
            Self::DeepSeek
        } else if model_lower.contains("mock") {
            Self::Mock
        } else {
            Self::None
        }
    }
}

/// Workflow execution phase for status feedback
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowPhase {
    #[default]
    Idle,
    Parsing,
    Validating,
    Executing,
    Completed,
    Failed,
}

impl WorkflowPhase {
    /// Get phase icon
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Idle => "○",
            Self::Parsing => "◐",
            Self::Validating => "◔",
            Self::Executing => "◑",
            Self::Completed => "●",
            Self::Failed => "⊗",
        }
    }

    /// Get animated icon for active phases
    pub fn animated_icon(&self, frame: u8) -> &'static str {
        match self {
            Self::Parsing | Self::Validating | Self::Executing => {
                // Spinning animation: ◐ ◓ ◑ ◒
                const SPIN: [&str; 4] = ["◐", "◓", "◑", "◒"];
                SPIN[(frame / 8) as usize % 4]
            }
            _ => self.icon(),
        }
    }

    /// Get phase display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Parsing => "Parsing",
            Self::Validating => "Validating",
            Self::Executing => "Executing",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
        }
    }

    /// Check if phase is active (showing animation)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Parsing | Self::Validating | Self::Executing)
    }
}

/// MCP Connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

impl ConnectionStatus {
    /// Get status icon
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Disconnected => "○",
            Self::Connecting => "◔",
            Self::Connected => "●",
            Self::Error => "⊗",
        }
    }

    /// Get animated icon (for connecting status)
    pub fn animated_icon(&self, frame: u8) -> &'static str {
        match self {
            Self::Connecting => {
                // Spinning animation: ◐ ◓ ◑ ◒
                const SPIN: [&str; 4] = ["◐", "◓", "◑", "◒"];
                SPIN[(frame / 8) as usize % 4]
            }
            _ => self.icon(),
        }
    }
}

/// Metrics to display on the right side of status bar
#[derive(Debug, Clone, Default)]
pub struct StatusMetrics {
    /// Current LLM provider
    pub provider: Provider,
    /// Total tokens used in session
    pub tokens: Option<u64>,
    /// Cache-read tokens (prompt caching)
    pub cache_tokens: u64,
    /// Number of connected MCP servers
    pub mcp_connected: usize,
    /// Total MCP servers configured
    pub mcp_total: usize,
    /// Connection status
    pub connection: ConnectionStatus,
    /// Current workflow phase
    pub phase: WorkflowPhase,
    /// Progress percentage (0-100) for long-running tasks
    pub progress: Option<u8>,
    /// Error code (NIKA-XXX) for failed state
    pub error_code: Option<String>,
}

impl StatusMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provider(mut self, provider: Provider) -> Self {
        self.provider = provider;
        self
    }

    pub fn tokens(mut self, tokens: u64) -> Self {
        self.tokens = Some(tokens);
        self
    }

    pub fn cache_tokens(mut self, cache_tokens: u64) -> Self {
        self.cache_tokens = cache_tokens;
        self
    }

    pub fn mcp(mut self, connected: usize, total: usize) -> Self {
        self.mcp_connected = connected;
        self.mcp_total = total;
        self
    }

    pub fn connection(mut self, status: ConnectionStatus) -> Self {
        self.connection = status;
        self
    }

    pub fn phase(mut self, phase: WorkflowPhase) -> Self {
        self.phase = phase;
        self
    }

    pub fn progress(mut self, progress: u8) -> Self {
        self.progress = Some(progress.min(100));
        self
    }

    pub fn error_code(mut self, code: impl Into<String>) -> Self {
        self.error_code = Some(code.into());
        self
    }

    /// Format progress as a mini bar
    fn format_progress(&self) -> Option<String> {
        self.progress.map(|p| {
            // Mini progress bar: [████░░░░░░] 45%
            let filled = (p as usize * 8) / 100;
            let empty = 8 - filled;
            format!("[{}{}] {}%", "█".repeat(filled), "░".repeat(empty), p)
        })
    }

    /// Format token count for display
    fn format_tokens(&self) -> Option<String> {
        self.tokens.map(|t| {
            if t >= 1_000_000 {
                format!("{:.1}M", t as f64 / 1_000_000.0)
            } else if t >= 1_000 {
                format!("{:.1}k", t as f64 / 1_000.0)
            } else {
                format!("{}", t)
            }
        })
    }
}

/// Severity level for status bar messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusSeverity {
    /// Normal informational text
    #[default]
    Info,
    /// Warning — shown in amber
    Warning,
    /// Error — shown in red bold, takes priority over hints
    Error,
}

/// Status bar configuration
pub struct StatusBar<'a> {
    /// Current view (determines which hints to show)
    pub view: TuiView,
    /// Optional custom hints (overrides defaults)
    pub hints: Option<Vec<KeyHint>>,
    /// Theme for colors
    pub theme: &'a Theme,
    /// Metrics to display on the right
    pub metrics: Option<StatusMetrics>,
    /// Current input mode (Normal, Insert, Command, Search)
    pub input_mode: Option<InputMode>,
    /// Custom status text from the view's status_line() method
    pub custom_text: Option<String>,
    /// Severity of the custom status text — Error/Warning get colored and prioritized
    pub status_severity: StatusSeverity,
    /// Animation frame counter (0-255) for live indicators
    pub frame: u8,
}

impl<'a> StatusBar<'a> {
    pub fn new(view: TuiView, theme: &'a Theme) -> Self {
        Self {
            view,
            hints: None,
            theme,
            metrics: None,
            input_mode: None,
            custom_text: None,
            status_severity: StatusSeverity::Info,
            frame: 0,
        }
    }

    /// Set animation frame for live indicators
    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    pub fn hints(mut self, hints: Vec<KeyHint>) -> Self {
        self.hints = Some(hints);
        self
    }

    pub fn metrics(mut self, metrics: StatusMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn mode(mut self, mode: InputMode) -> Self {
        self.input_mode = Some(mode);
        self
    }

    /// Set custom status text from the view's status_line() method
    pub fn custom_text(mut self, text: String) -> Self {
        if !text.is_empty() {
            // PERF: Detect severity without to_lowercase() allocation.
            let has_error = text.contains("error")
                || text.contains("Error")
                || text.contains("ERROR")
                || text.starts_with("NIKA-")
                || text.contains("failed")
                || text.contains("Failed");
            let has_warning = text.contains("warning")
                || text.contains("Warning")
                || text.contains("disconnected")
                || text.contains("Disconnected")
                || text.contains("retrying")
                || text.contains("Retrying");
            if has_error {
                self.status_severity = StatusSeverity::Error;
            } else if has_warning {
                self.status_severity = StatusSeverity::Warning;
            }
            self.custom_text = Some(text);
        }
        self
    }

    /// Explicitly set status severity
    pub fn severity(mut self, severity: StatusSeverity) -> Self {
        self.status_severity = severity;
        self
    }

    fn default_hints(&self) -> Vec<KeyHint> {
        // If input_mode is set, use keybindings_for_context for dynamic hints
        if let Some(mode) = self.input_mode {
            return self.hints_from_keybindings(keybindings_for_context(self.view, mode));
        }

        // Fallback to static hints
        match self.view {
            TuiView::Command => vec![
                KeyHint::new("Enter", "Send"),
                KeyHint::new("Up/Down", "History"),
                KeyHint::new("Tab", "Views"),
                KeyHint::new("Ctrl+L", "Clear"),
                KeyHint::new("q", "Quit"),
            ],
            TuiView::Studio => vec![
                KeyHint::new("i", "Insert"),
                KeyHint::new("Esc", "Normal"),
                KeyHint::new("F5", "Run"),
                KeyHint::new("Ctrl+S", "Save"),
                KeyHint::new("c", "Command"),
                KeyHint::new("q", "Back"),
            ],
            // Control is auxiliary view
            TuiView::Control => vec![
                KeyHint::new("Tab", "Next"),
                KeyHint::new("Enter", "Select"),
                KeyHint::new("Esc", "Back"),
                KeyHint::new("q", "Close"),
            ],
        }
    }

    /// Convert keybindings to key hints for display
    ///
    /// Selects the most relevant keybindings based on priority:
    /// - View-specific actions first
    /// - Mode switching actions
    /// - Global actions (quit, help)
    fn hints_from_keybindings(&self, keybindings: Vec<Keybinding>) -> Vec<KeyHint> {
        // Prioritize categories: view-specific > mode > scroll > global
        let priority = |kb: &Keybinding| -> u8 {
            match kb.category {
                KeyCategory::Chat | KeyCategory::Monitor => 0,
                KeyCategory::Action => 1,
                KeyCategory::Mode => 2,
                KeyCategory::Scroll => 3,
                KeyCategory::PanelNav => 4,
                KeyCategory::ViewNav => 5,
                KeyCategory::Global => 6,
            }
        };

        // Sort by priority and take top 6 hints (to fit status bar)
        let mut sorted: Vec<_> = keybindings.iter().collect();
        sorted.sort_by_key(|kb| priority(kb));

        sorted
            .into_iter()
            .take(6)
            .map(|kb| {
                // Convert Keybinding to KeyHint using format_key
                let key_str = format_key(kb.code, kb.modifiers);
                // Use with_owned_key to avoid Box::leak memory leak
                KeyHint::with_owned_key(key_str, kb.description)
            })
            .collect()
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Fill the entire row with the status bar background first
        let header_bg = self.theme.header_bg;
        let bg_style = Style::default().bg(header_bg);
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_style(bg_style);
        }

        // Get default hints before potentially moving self.hints
        let default = self.default_hints();
        let hints = self.hints.unwrap_or(default);

        // Build left side (input mode indicator + key hints)
        let mut left_spans = vec![Span::raw(" ")];

        // Add input mode indicator if set — PERF: static strings (zero format!)
        if let Some(mode) = self.input_mode {
            let (mode_label, mode_color) = match mode {
                InputMode::Normal => ("[N]", self.theme.status_success),
                InputMode::Insert => ("[I]", self.theme.status_running),
                InputMode::Search => ("[/]", self.theme.highlight),
            };
            left_spans.push(Span::styled(
                mode_label,
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ));
            left_spans.push(Span::raw(" "));
        }

        // Add custom status text from view (if present)
        // Error/Warning severity gets colored and bold for visibility
        let had_custom_text = self.custom_text.is_some();
        if let Some(text) = self.custom_text {
            let text_style = match self.status_severity {
                StatusSeverity::Error => Style::default()
                    .fg(self.theme.status_failed)
                    .add_modifier(Modifier::BOLD),
                StatusSeverity::Warning => Style::default()
                    .fg(self.theme.status_running) // amber
                    .add_modifier(Modifier::BOLD),
                StatusSeverity::Info => Style::default().fg(self.theme.text_primary),
            };
            // Error/Warning prefix icon
            match self.status_severity {
                StatusSeverity::Error => {
                    left_spans.push(Span::styled(
                        "\u{2717} ",
                        Style::default()
                            .fg(self.theme.status_failed)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                StatusSeverity::Warning => {
                    left_spans.push(Span::styled(
                        "\u{26a0} ",
                        Style::default()
                            .fg(self.theme.status_running)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                StatusSeverity::Info => {}
            }
            left_spans.push(Span::styled(text, text_style));
            left_spans.push(Span::styled(
                " \u{2502} ",
                Style::default().fg(self.theme.text_muted),
            ));
        }

        // Hints are added after metrics width is known (progressive disclosure below)

        // Build right side (metrics) if available
        let mut right_spans: Vec<Span> = Vec::new();

        if let Some(ref metrics) = self.metrics {
            // Workflow phase indicator (only show if not Idle)
            if metrics.phase != WorkflowPhase::Idle {
                // Phase color based on state
                let phase_color = match metrics.phase {
                    WorkflowPhase::Idle => self.theme.text_muted,
                    WorkflowPhase::Parsing | WorkflowPhase::Validating => {
                        // Animated pulse between theme parsing colors
                        if (self.frame / 8) % 2 == 0 {
                            self.theme.status_parsing_a
                        } else {
                            self.theme.status_parsing_b
                        }
                    }
                    WorkflowPhase::Executing => {
                        // Animated pulse between theme executing colors
                        if (self.frame / 8) % 2 == 0 {
                            self.theme.status_executing_a
                        } else {
                            self.theme.status_executing_b
                        }
                    }
                    WorkflowPhase::Completed => self.theme.status_success,
                    WorkflowPhase::Failed => self.theme.status_failed,
                };

                right_spans.push(Span::styled(
                    metrics.phase.animated_icon(self.frame),
                    Style::default().fg(phase_color),
                ));
                right_spans.push(Span::raw(" "));
                right_spans.push(Span::styled(
                    metrics.phase.name(),
                    Style::default().fg(phase_color),
                ));

                // Show progress bar if present
                if let Some(progress_str) = metrics.format_progress() {
                    right_spans.push(Span::raw(" "));
                    right_spans.push(Span::styled(
                        progress_str,
                        Style::default().fg(self.theme.highlight),
                    ));
                }

                // Show error code if failed
                if metrics.phase == WorkflowPhase::Failed {
                    if let Some(ref code) = metrics.error_code {
                        let error_style = Style::default()
                            .fg(self.theme.status_failed)
                            .add_modifier(Modifier::BOLD);
                        right_spans.push(Span::raw(" "));
                        right_spans.push(Span::styled("[", error_style));
                        right_spans.push(Span::styled(code.as_str(), error_style));
                        right_spans.push(Span::styled("]", error_style));
                    }
                }
            }

            // Provider indicator
            if metrics.provider != Provider::None {
                if !right_spans.is_empty() {
                    right_spans.push(Span::styled(
                        " │ ",
                        Style::default().fg(self.theme.text_muted),
                    ));
                }
                right_spans.push(Span::raw(metrics.provider.icon()));
                right_spans.push(Span::raw(" "));
                right_spans.push(Span::styled(
                    metrics.provider.name(),
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            // Token counter
            if let Some(token_str) = metrics.format_tokens() {
                if !right_spans.is_empty() {
                    right_spans.push(Span::styled(
                        " │ ",
                        Style::default().fg(self.theme.text_muted),
                    ));
                }
                right_spans.push(Span::styled(
                    token_str,
                    Style::default().fg(self.theme.status_running), // Amber for attention
                ));
                right_spans.push(Span::styled(
                    " tokens",
                    Style::default().fg(self.theme.text_secondary),
                ));
                // Show cache-read tokens when present
                if metrics.cache_tokens > 0 {
                    let cache_str = if metrics.cache_tokens >= 1_000 {
                        format!(" (cache: {:.1}k)", metrics.cache_tokens as f64 / 1_000.0)
                    } else {
                        format!(" (cache: {})", metrics.cache_tokens)
                    };
                    right_spans.push(Span::styled(
                        cache_str,
                        Style::default().fg(self.theme.text_muted),
                    ));
                }
            }

            // MCP connection status
            if metrics.mcp_total > 0 {
                if !right_spans.is_empty() {
                    right_spans.push(Span::styled(
                        " │ ",
                        Style::default().fg(self.theme.text_muted),
                    ));
                }

                // Connection status icon with color (animated for Connecting)
                let conn_color = match metrics.connection {
                    ConnectionStatus::Connected => self.theme.status_success,
                    ConnectionStatus::Connecting => {
                        // Animated pulse between theme executing colors
                        if (self.frame / 8) % 2 == 0 {
                            self.theme.status_executing_a
                        } else {
                            self.theme.status_executing_b
                        }
                    }
                    ConnectionStatus::Disconnected => self.theme.text_muted,
                    ConnectionStatus::Error => self.theme.status_failed,
                };

                right_spans.push(Span::styled(
                    metrics.connection.animated_icon(self.frame),
                    Style::default().fg(conn_color),
                ));
                right_spans.push(Span::raw(" "));
                right_spans.push(Span::styled(
                    "MCP:",
                    Style::default().fg(self.theme.text_secondary),
                ));
                right_spans.push(Span::styled(
                    format!(" {}/{}", metrics.mcp_connected, metrics.mcp_total),
                    Style::default().fg(if metrics.mcp_connected == metrics.mcp_total {
                        self.theme.status_success
                    } else {
                        self.theme.text_primary
                    }),
                ));
            }

            right_spans.push(Span::raw(" "));
        }

        // Progressive disclosure: metrics (right) always render,
        // hints (left) get dropped from lowest priority when space is tight
        let right_width: usize = right_spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        let left_base_width: usize = left_spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        let available_for_hints = (area.width as usize)
            .saturating_sub(right_width)
            .saturating_sub(left_base_width)
            .saturating_sub(2); // padding

        // Only add hints that fit in remaining space
        let mut hints_width = 0usize;
        for (i, hint) in hints.iter().enumerate() {
            let hint_width = if i > 0 || self.input_mode.is_some() || had_custom_text {
                2 // leading "  "
            } else {
                0
            } + UnicodeWidthStr::width(hint.key.as_ref())
                + 2 // brackets []
                + 1 // space
                + UnicodeWidthStr::width(hint.action.as_ref());
            if hints_width + hint_width > available_for_hints {
                break; // No more room
            }
            hints_width += hint_width;

            if i > 0 || self.input_mode.is_some() || had_custom_text {
                left_spans.push(Span::raw("  "));
            }
            // PERF: 3 spans instead of format!("[{}]", key) — zero alloc for static keys
            let key_style = Style::default()
                .fg(self.theme.highlight)
                .add_modifier(Modifier::BOLD);
            left_spans.push(Span::styled("[", key_style));
            left_spans.push(Span::styled(hint.key.clone(), key_style));
            left_spans.push(Span::styled("]", key_style));
            left_spans.push(Span::raw(" "));
            // PERF: Pass Cow directly — avoids to_string() allocation for static hints
            left_spans.push(Span::styled(
                hint.action.clone(),
                Style::default().fg(self.theme.text_secondary),
            ));
        }

        // Render left side
        let left_line = Line::from(left_spans);
        let left_paragraph = Paragraph::new(left_line).style(Style::default().bg(header_bg));
        left_paragraph.render(area, buf);

        // Render right side (right-aligned) — always shown if space exists
        let left_width = left_base_width + hints_width;
        if !right_spans.is_empty() && area.width as usize > left_width + right_width {
            let right_x = area.x + area.width - right_width as u16;
            let right_line = Line::from(right_spans);

            for (i, span) in right_line.spans.iter().enumerate() {
                let x_offset: usize = right_line.spans[..i]
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum();
                buf.set_string(
                    right_x + x_offset as u16,
                    area.y,
                    span.content.as_ref(),
                    span.style,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_default_hints_studio() {
        let theme = Theme::dark();
        let bar = StatusBar::new(TuiView::Studio, &theme);
        let hints = bar.default_hints();
        // Studio view hints include editor-style shortcuts
        assert!(hints
            .iter()
            .any(|h| h.key.as_ref() == "F5" && h.action.as_ref() == "Run"));
        assert!(hints
            .iter()
            .any(|h| h.key.as_ref() == "Ctrl+S" && h.action.as_ref() == "Save"));
    }

    #[test]
    fn test_status_bar_custom_hints() {
        let theme = Theme::dark();
        let custom = vec![KeyHint::new("x", "Custom")];
        let bar = StatusBar::new(TuiView::Command, &theme).hints(custom);
        assert!(bar.hints.is_some());
        assert_eq!(bar.hints.unwrap().len(), 1);
    }

    #[test]
    fn test_provider_icons_match_canonical() {
        use crate::icons::provider as p;
        assert_eq!(Provider::Claude.icon(), p::CLAUDE);
        assert_eq!(Provider::OpenAI.icon(), p::OPENAI);
        assert_eq!(Provider::Mistral.icon(), p::MISTRAL);
        assert_eq!(Provider::Native.icon(), p::NATIVE);
        assert_eq!(Provider::Groq.icon(), p::GROQ);
        assert_eq!(Provider::DeepSeek.icon(), p::DEEPSEEK);
        assert_eq!(Provider::Mock.icon(), p::MOCK);
        assert_eq!(Provider::None.icon(), "  ");
    }

    #[test]
    fn test_provider_names() {
        assert_eq!(Provider::Claude.name(), "Claude");
        assert_eq!(Provider::OpenAI.name(), "OpenAI");
        assert_eq!(Provider::Mistral.name(), "Mistral");
        assert_eq!(Provider::Native.name(), "Native");
        assert_eq!(Provider::Groq.name(), "Groq");
        assert_eq!(Provider::DeepSeek.name(), "DeepSeek");
        assert_eq!(Provider::Mock.name(), "Mock");
        assert_eq!(Provider::None.name(), "---");
    }

    #[test]
    fn test_connection_status_icons() {
        assert_eq!(ConnectionStatus::Connected.icon(), "●");
        assert_eq!(ConnectionStatus::Connecting.icon(), "◔");
        assert_eq!(ConnectionStatus::Disconnected.icon(), "○");
        assert_eq!(ConnectionStatus::Error.icon(), "⊗");
    }

    #[test]
    fn test_status_metrics_token_formatting() {
        let m1 = StatusMetrics::new().tokens(500);
        assert_eq!(m1.format_tokens(), Some("500".to_string()));

        let m2 = StatusMetrics::new().tokens(1500);
        assert_eq!(m2.format_tokens(), Some("1.5k".to_string()));

        let m3 = StatusMetrics::new().tokens(1_500_000);
        assert_eq!(m3.format_tokens(), Some("1.5M".to_string()));

        let m4 = StatusMetrics::new();
        assert_eq!(m4.format_tokens(), None);
    }

    #[test]
    fn test_status_metrics_builder() {
        let metrics = StatusMetrics::new()
            .provider(Provider::Claude)
            .tokens(1234)
            .mcp(2, 3)
            .connection(ConnectionStatus::Connected);

        assert_eq!(metrics.provider, Provider::Claude);
        assert_eq!(metrics.tokens, Some(1234));
        assert_eq!(metrics.mcp_connected, 2);
        assert_eq!(metrics.mcp_total, 3);
        assert_eq!(metrics.connection, ConnectionStatus::Connected);
    }

    #[test]
    fn test_status_bar_with_metrics() {
        let theme = Theme::dark();
        let metrics = StatusMetrics::new().provider(Provider::Claude).tokens(5000);
        let bar = StatusBar::new(TuiView::Command, &theme).metrics(metrics);
        assert!(bar.metrics.is_some());
    }

    #[test]
    fn test_workflow_phase_icons() {
        assert_eq!(WorkflowPhase::Idle.icon(), "○");
        assert_eq!(WorkflowPhase::Parsing.icon(), "◐");
        assert_eq!(WorkflowPhase::Validating.icon(), "◔");
        assert_eq!(WorkflowPhase::Executing.icon(), "◑");
        assert_eq!(WorkflowPhase::Completed.icon(), "●");
        assert_eq!(WorkflowPhase::Failed.icon(), "⊗");
    }

    #[test]
    fn test_workflow_phase_names() {
        assert_eq!(WorkflowPhase::Idle.name(), "Idle");
        assert_eq!(WorkflowPhase::Parsing.name(), "Parsing");
        assert_eq!(WorkflowPhase::Validating.name(), "Validating");
        assert_eq!(WorkflowPhase::Executing.name(), "Executing");
        assert_eq!(WorkflowPhase::Completed.name(), "Completed");
        assert_eq!(WorkflowPhase::Failed.name(), "Failed");
    }

    #[test]
    fn test_workflow_phase_is_active() {
        assert!(!WorkflowPhase::Idle.is_active());
        assert!(WorkflowPhase::Parsing.is_active());
        assert!(WorkflowPhase::Validating.is_active());
        assert!(WorkflowPhase::Executing.is_active());
        assert!(!WorkflowPhase::Completed.is_active());
        assert!(!WorkflowPhase::Failed.is_active());
    }

    #[test]
    fn test_workflow_phase_animated_icons() {
        // Active phases should return spinning animation
        let parsing_icon = WorkflowPhase::Parsing.animated_icon(0);
        assert!(["◐", "◓", "◑", "◒"].contains(&parsing_icon));

        // Non-active phases return static icon
        assert_eq!(WorkflowPhase::Idle.animated_icon(0), "○");
        assert_eq!(WorkflowPhase::Completed.animated_icon(0), "●");
        assert_eq!(WorkflowPhase::Failed.animated_icon(0), "⊗");
    }

    #[test]
    fn test_status_metrics_progress_formatting() {
        let m1 = StatusMetrics::new().progress(0);
        assert_eq!(m1.format_progress(), Some("[░░░░░░░░] 0%".to_string()));

        let m2 = StatusMetrics::new().progress(50);
        assert_eq!(m2.format_progress(), Some("[████░░░░] 50%".to_string()));

        let m3 = StatusMetrics::new().progress(100);
        assert_eq!(m3.format_progress(), Some("[████████] 100%".to_string()));

        let m4 = StatusMetrics::new(); // No progress set
        assert_eq!(m4.format_progress(), None);
    }

    #[test]
    fn test_status_metrics_progress_clamping() {
        // Progress should be clamped to 100
        let m = StatusMetrics::new().progress(150);
        assert_eq!(m.progress, Some(100));
    }

    #[test]
    fn test_status_metrics_phase_builder() {
        let metrics = StatusMetrics::new()
            .phase(WorkflowPhase::Executing)
            .progress(75)
            .error_code("NIKA-042");

        assert_eq!(metrics.phase, WorkflowPhase::Executing);
        assert_eq!(metrics.progress, Some(75));
        assert_eq!(metrics.error_code, Some("NIKA-042".to_string()));
    }

    #[test]
    fn test_status_metrics_default_phase() {
        let metrics = StatusMetrics::new();
        assert_eq!(metrics.phase, WorkflowPhase::Idle);
        assert_eq!(metrics.progress, None);
        assert_eq!(metrics.error_code, None);
    }
}
