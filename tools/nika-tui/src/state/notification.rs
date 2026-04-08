// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Notification System Types
//!
//! Types for the TUI notification system (TIER 3.4).

// ═══════════════════════════════════════════
// NOTIFICATION LEVEL
// ═══════════════════════════════════════════

/// Notification severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    /// Informational (workflow complete, etc.)
    Info,
    /// Warning (task slow, high token usage)
    Warning,
    /// Alert (critical issues)
    Alert,
    /// Success (workflow completed successfully)
    Success,
    /// Error (workflow failed)
    Error,
}

impl NotificationLevel {
    /// Get icon for this level
    pub fn icon(&self) -> &'static str {
        match self {
            NotificationLevel::Info => "ℹ",
            NotificationLevel::Warning => "⚠",
            NotificationLevel::Alert => "🔔",
            NotificationLevel::Success => "✓",
            NotificationLevel::Error => "✗",
        }
    }
}

// ═══════════════════════════════════════════
// NOTIFICATION
// ═══════════════════════════════════════════

/// A system notification (TIER 3.4)
#[derive(Debug, Clone)]
pub struct Notification {
    /// Notification level/severity
    pub level: NotificationLevel,
    /// Notification message
    pub message: String,
    /// Timestamp (ms since workflow start)
    pub timestamp_ms: u64,
    /// Whether this notification has been dismissed
    pub dismissed: bool,
}

impl Notification {
    /// Create a new notification
    pub fn new(level: NotificationLevel, message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self {
            level,
            message: message.into(),
            timestamp_ms,
            dismissed: false,
        }
    }

    /// Create an info notification
    pub fn info(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Info, message, timestamp_ms)
    }

    /// Create a warning notification
    pub fn warning(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Warning, message, timestamp_ms)
    }

    /// Create an alert notification
    pub fn alert(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Alert, message, timestamp_ms)
    }

    /// Create a success notification
    pub fn success(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Success, message, timestamp_ms)
    }

    /// Create an error notification
    pub fn error(message: impl Into<String>, timestamp_ms: u64) -> Self {
        Self::new(NotificationLevel::Error, message, timestamp_ms)
    }
}
