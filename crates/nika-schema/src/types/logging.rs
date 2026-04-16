// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Logging configuration — the `log:` section at workflow root.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Log output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    /// Plain text logs.
    #[default]
    Text,
    /// Structured JSON logs.
    Json,
    /// JSON Lines (one JSON object per line).
    Jsonl,
}

impl fmt::Display for LogFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::Jsonl => write!(f, "jsonl"),
        }
    }
}

/// Log severity level.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Detailed debugging information.
    Debug,
    /// General operational information.
    #[default]
    Info,
    /// Warning conditions.
    Warn,
    /// Error conditions.
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Logging configuration for a workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LogConfig {
    /// Log level filter.
    #[serde(default)]
    pub level: LogLevel,
    /// Output format.
    #[serde(default)]
    pub format: LogFormat,
    /// Optional file path for log output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Whether to include timestamps.
    #[serde(default = "default_true")]
    pub timestamps: bool,
}

fn default_true() -> bool {
    true
}

impl LogConfig {
    /// Create a new log configuration with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            level: LogLevel::default(),
            format: LogFormat::default(),
            file: None,
            timestamps: true,
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn log_format_display() {
        assert_eq!(LogFormat::Jsonl.to_string(), "jsonl");
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = LogConfig {
            level: LogLevel::Debug,
            format: LogFormat::Json,
            file: Some("/tmp/nika.log".into()),
            timestamps: false,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: LogConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.level, LogLevel::Debug);
        assert_eq!(back.format, LogFormat::Json);
        assert_eq!(back.file.as_deref(), Some("/tmp/nika.log"));
        assert!(!back.timestamps);
    }

    #[test]
    fn default_has_timestamps() {
        let cfg = LogConfig::new();
        assert!(cfg.timestamps);
        assert_eq!(cfg.level, LogLevel::Info);
    }
}
