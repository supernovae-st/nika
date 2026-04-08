// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Status Message Widget — User feedback system
//!
//! Provides visual feedback for user actions with auto-dismiss.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │ ✓ Message copied to clipboard                    [3s]  │
//! └─────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

// Status colors
const COLOR_INFO: Color = Color::Rgb(56, 189, 248); // Sky blue
const COLOR_SUCCESS: Color = Color::Rgb(74, 222, 128); // Green
const COLOR_WARNING: Color = Color::Rgb(251, 191, 36); // Amber
const COLOR_ERROR: Color = Color::Rgb(248, 113, 113); // Red

/// Status message severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl StatusLevel {
    /// Get icon for status level
    pub fn icon(&self) -> &'static str {
        match self {
            StatusLevel::Info => "ℹ",
            StatusLevel::Success => "✓",
            StatusLevel::Warning => "⚠",
            StatusLevel::Error => "✗",
        }
    }

    /// Get color for status level
    pub fn color(&self) -> Color {
        match self {
            StatusLevel::Info => COLOR_INFO,
            StatusLevel::Success => COLOR_SUCCESS,
            StatusLevel::Warning => COLOR_WARNING,
            StatusLevel::Error => COLOR_ERROR,
        }
    }

    /// Get default duration for this level
    pub fn default_duration(&self) -> Duration {
        match self {
            StatusLevel::Info => Duration::from_secs(3),
            StatusLevel::Success => Duration::from_secs(2),
            StatusLevel::Warning => Duration::from_secs(4),
            StatusLevel::Error => Duration::from_secs(5),
        }
    }
}

/// A status message with auto-dismiss
#[derive(Debug, Clone, PartialEq)]
pub struct StatusMessage {
    /// Severity level
    pub level: StatusLevel,
    /// Message text
    pub message: String,
    /// When the message was shown
    pub timestamp: Instant,
    /// How long to show (None = manual dismiss)
    pub duration: Option<Duration>,
}

impl StatusMessage {
    /// Create a new status message with default duration
    pub fn new(level: StatusLevel, message: impl Into<String>) -> Self {
        let duration = Some(level.default_duration());
        Self {
            level,
            message: message.into(),
            timestamp: Instant::now(),
            duration,
        }
    }

    /// Create an info message
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(StatusLevel::Info, message)
    }

    /// Create a success message
    pub fn success(message: impl Into<String>) -> Self {
        Self::new(StatusLevel::Success, message)
    }

    /// Create a warning message
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(StatusLevel::Warning, message)
    }

    /// Create an error message
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(StatusLevel::Error, message)
    }

    /// Create a persistent message (no auto-dismiss)
    pub fn persistent(level: StatusLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            timestamp: Instant::now(),
            duration: None,
        }
    }

    /// Set custom duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        match self.duration {
            Some(d) => self.timestamp.elapsed() > d,
            None => false, // Persistent
        }
    }

    /// Get remaining time (for display)
    pub fn remaining_secs(&self) -> Option<u64> {
        self.duration.map(|d| {
            let elapsed = self.timestamp.elapsed();
            d.saturating_sub(elapsed).as_secs()
        })
    }
}

/// Status message queue for managing multiple messages
#[derive(Debug, Default, Clone)]
pub struct StatusQueue {
    messages: VecDeque<StatusMessage>,
    max_messages: usize,
}

impl StatusQueue {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            max_messages: 5,
        }
    }

    /// Push a new status message
    pub fn push(&mut self, message: StatusMessage) {
        self.messages.push_back(message);
        // Trim to max
        while self.messages.len() > self.max_messages {
            self.messages.pop_front();
        }
    }

    /// Push convenience methods
    pub fn info(&mut self, message: impl Into<String>) {
        self.push(StatusMessage::info(message));
    }

    pub fn success(&mut self, message: impl Into<String>) {
        self.push(StatusMessage::success(message));
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        self.push(StatusMessage::warning(message));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.push(StatusMessage::error(message));
    }

    /// Remove expired messages
    pub fn tick(&mut self) {
        self.messages.retain(|m| !m.is_expired());
    }

    /// Get current (most recent non-expired) message
    pub fn current(&self) -> Option<&StatusMessage> {
        self.messages.iter().rev().find(|m| !m.is_expired())
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.messages.iter().all(|m| m.is_expired())
    }
}

/// Widget to display the current status message
pub struct StatusMessageWidget<'a> {
    message: Option<&'a StatusMessage>,
}

impl<'a> StatusMessageWidget<'a> {
    pub fn new(message: Option<&'a StatusMessage>) -> Self {
        Self { message }
    }

    pub fn from_queue(queue: &'a StatusQueue) -> Self {
        Self::new(queue.current())
    }
}

impl Widget for StatusMessageWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(msg) = self.message else {
            return;
        };

        if area.width < 10 || area.height < 1 {
            return;
        }

        let color = msg.level.color();
        let icon = msg.level.icon();

        // Build message line
        let mut spans = vec![
            Span::styled(
                format!(" {} ", icon),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(&msg.message, Style::default().fg(color)),
        ];

        // Add countdown if applicable
        if let Some(secs) = msg.remaining_secs() {
            if secs > 0 {
                spans.push(Span::styled(
                    format!("  [{}s]", secs),
                    Style::default()
                        .fg(Color::Rgb(107, 114, 128))
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).alignment(Alignment::Left);

        // Render with subtle background
        let bg_style = Style::default().bg(Color::Rgb(8, 8, 14));
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_style(bg_style);
            }
        }

        paragraph.render(area, buf);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_level_icon() {
        assert_eq!(StatusLevel::Info.icon(), "ℹ");
        assert_eq!(StatusLevel::Success.icon(), "✓");
        assert_eq!(StatusLevel::Warning.icon(), "⚠");
        assert_eq!(StatusLevel::Error.icon(), "✗");
    }

    #[test]
    fn test_status_level_default_duration() {
        assert_eq!(StatusLevel::Info.default_duration(), Duration::from_secs(3));
        assert_eq!(
            StatusLevel::Success.default_duration(),
            Duration::from_secs(2)
        );
        assert_eq!(
            StatusLevel::Warning.default_duration(),
            Duration::from_secs(4)
        );
        assert_eq!(
            StatusLevel::Error.default_duration(),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn test_status_message_constructors() {
        let info = StatusMessage::info("Test info");
        assert_eq!(info.level, StatusLevel::Info);
        assert_eq!(info.message, "Test info");

        let success = StatusMessage::success("Saved");
        assert_eq!(success.level, StatusLevel::Success);

        let warning = StatusMessage::warning("Caution");
        assert_eq!(warning.level, StatusLevel::Warning);

        let error = StatusMessage::error("Failed");
        assert_eq!(error.level, StatusLevel::Error);
    }

    #[test]
    fn test_status_message_persistent() {
        let msg = StatusMessage::persistent(StatusLevel::Info, "Loading...");
        assert!(msg.duration.is_none());
        assert!(!msg.is_expired());
    }

    #[test]
    fn test_status_message_with_duration() {
        let msg = StatusMessage::info("Test").with_duration(Duration::from_secs(10));
        assert_eq!(msg.duration, Some(Duration::from_secs(10)));
    }

    #[test]
    fn test_status_message_expiry() {
        // Fresh message should not be expired
        let msg = StatusMessage::new(StatusLevel::Success, "Test");
        assert!(!msg.is_expired());

        // Persistent message never expires
        let persistent = StatusMessage::persistent(StatusLevel::Info, "Loading");
        assert!(!persistent.is_expired());
    }

    #[test]
    fn test_status_queue_push() {
        let mut queue = StatusQueue::new();
        assert!(queue.is_empty());

        queue.info("First");
        queue.success("Second");

        assert!(!queue.is_empty());
        assert_eq!(queue.current().unwrap().message, "Second");
    }

    #[test]
    fn test_status_queue_max_messages() {
        let mut queue = StatusQueue::new();
        queue.max_messages = 3;

        queue.info("1");
        queue.info("2");
        queue.info("3");
        queue.info("4");

        assert_eq!(queue.messages.len(), 3);
        assert_eq!(queue.messages[0].message, "2");
    }

    #[test]
    fn test_status_queue_clear() {
        let mut queue = StatusQueue::new();
        queue.info("Test");
        queue.clear();
        assert!(queue.messages.is_empty());
    }

    #[test]
    fn test_status_message_remaining_secs() {
        let msg = StatusMessage::info("Test").with_duration(Duration::from_secs(5));
        let remaining = msg.remaining_secs();
        assert!(remaining.is_some());
        assert!(remaining.unwrap() <= 5);
    }
}
