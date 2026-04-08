// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Logging configuration
//!
//! Provides level-filtered logging for workflows and tasks.
//!
//! # YAML Syntax
//!
//! ```yaml
//! # Workflow-level defaults
//! log:
//!   level: info
//!   console: true
//!   file: ./logs/{{context.meta.workflow}}.log
//!
//! tasks:
//!   # Task-specific override
//!   - id: verbose_task
//!     log:
//!       level: debug
//!       console: false
//!       file: ./logs/verbose.log
//! ```

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// LOG FORMAT
// ═══════════════════════════════════════════════════════════════════════════

/// Log output format
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Plain text output (default)
    #[default]
    Text,

    /// JSON structured output
    Json,
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LOG CONFIG
// ═══════════════════════════════════════════════════════════════════════════

/// Logging configuration for workflow or task
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogConfig {
    /// Minimum log level
    #[serde(default)]
    pub level: LogLevel,

    /// Log output format (text or json)
    #[serde(default)]
    pub format: LogFormat,

    /// Show in console output
    #[serde(default = "default_true")]
    pub console: bool,

    /// Optional log file path (supports context.meta.* templates)
    #[serde(default)]
    pub file: Option<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::default(),
            format: LogFormat::default(),
            console: true,
            file: None,
        }
    }
}

fn default_true() -> bool {
    true
}

// ═══════════════════════════════════════════════════════════════════════════
// LOG LEVEL
// ═══════════════════════════════════════════════════════════════════════════

/// Log levels
///
/// Ordered from most verbose to least:
/// Trace < Debug < Info < Warn < Error
#[derive(
    Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Trace messages (most verbose)
    Trace,

    /// Debug messages
    Debug,

    /// Informational messages (default)
    #[default]
    Info,

    /// Warning messages
    Warn,

    /// Error messages (least verbose)
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace => write!(f, "trace"),
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl LogLevel {
    /// Check if this level should be logged given a minimum level
    ///
    /// # Example
    ///
    /// ```ignore
    /// let min = LogLevel::Warn;
    /// assert!(!LogLevel::Debug.should_log(min)); // Debug < Warn
    /// assert!(!LogLevel::Info.should_log(min));  // Info < Warn
    /// assert!(LogLevel::Warn.should_log(min));   // Warn >= Warn
    /// assert!(LogLevel::Error.should_log(min));  // Error > Warn
    /// ```
    pub fn should_log(&self, min_level: LogLevel) -> bool {
        *self >= min_level
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_parse_log_config_full() {
        let yaml = r#"
level: debug
console: false
file: ./logs/workflow.log
"#;
        let config: LogConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.level, LogLevel::Debug);
        assert!(!config.console);
        assert_eq!(config.file, Some("./logs/workflow.log".to_string()));
    }

    #[test]
    fn test_parse_log_config_minimal() {
        let yaml = "level: warn";
        let config: LogConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.level, LogLevel::Warn);
        assert!(config.console); // default true
        assert_eq!(config.file, None);
    }

    #[test]
    fn test_log_config_defaults() {
        let config = LogConfig::default();
        assert_eq!(config.level, LogLevel::Info);
        assert_eq!(config.format, LogFormat::Text);
        assert!(config.console);
        assert_eq!(config.file, None);
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_log_level_should_log() {
        let min = LogLevel::Warn;

        assert!(!LogLevel::Debug.should_log(min));
        assert!(!LogLevel::Info.should_log(min));
        assert!(LogLevel::Warn.should_log(min));
        assert!(LogLevel::Error.should_log(min));
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Trace.to_string(), "trace");
        assert_eq!(LogLevel::Debug.to_string(), "debug");
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Warn.to_string(), "warn");
        assert_eq!(LogLevel::Error.to_string(), "error");
    }

    #[test]
    fn test_log_format_display() {
        assert_eq!(LogFormat::Text.to_string(), "text");
        assert_eq!(LogFormat::Json.to_string(), "json");
    }

    #[test]
    fn test_parse_log_format_json() {
        let yaml = r#"
level: debug
format: json
console: true
"#;
        let config: LogConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.format, LogFormat::Json);
    }

    #[test]
    fn test_parse_log_level_trace() {
        let yaml = "level: trace";
        let config: LogConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.level, LogLevel::Trace);
    }

    #[test]
    fn test_trace_should_log() {
        // Trace is most verbose, should log at all levels
        assert!(LogLevel::Trace.should_log(LogLevel::Trace));
        assert!(!LogLevel::Trace.should_log(LogLevel::Debug));
        assert!(!LogLevel::Trace.should_log(LogLevel::Info));
    }

    #[test]
    fn test_log_config_with_template() {
        let yaml = r#"
level: info
file: ./logs/{{context.meta.workflow}}-{{context.meta.date}}.log
"#;
        let config: LogConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config
            .file
            .as_ref()
            .unwrap()
            .contains("context.meta.workflow"));
        assert!(config.file.as_ref().unwrap().contains("context.meta.date"));
    }
}
