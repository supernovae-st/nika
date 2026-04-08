// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:log - Emit log event at level.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "level": "info",      // trace, debug, info, warn, error
//!   "message": "Hello"    // Log message
//! }
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "logged": true,
//!   "level": "info",
//!   "message": "Hello"
//! }
//! ```

use super::BuiltinTool;
use crate::error::NikaError;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use tracing::{debug, error, info, trace, warn};

/// Valid log levels for nika:log tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Get the level name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = ();

    /// Parse log level from string (case-insensitive).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" | "warning" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(()),
        }
    }
}

/// Parameters for nika:log tool.
#[derive(Debug, Clone, Deserialize)]
struct LogParams {
    /// Log level: trace, debug, info, warn, error.
    level: String,
    /// Log message.
    message: String,
}

/// Response from nika:log tool.
#[derive(Debug, Clone, Serialize)]
struct LogResponse {
    /// Whether the log was emitted.
    logged: bool,
    /// The level that was logged.
    level: String,
    /// The message that was logged.
    message: String,
}

/// nika:log builtin tool.
///
/// Emits log events via tracing at the specified level.
pub struct LogTool;

impl BuiltinTool for LogTool {
    fn name(&self) -> &'static str {
        "log"
    }

    fn description(&self) -> &'static str {
        "Emit log event at specified level (trace, debug, info, warn, error)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        // OpenAI-compatible schema with additionalProperties: false
        serde_json::json!({
            "type": "object",
            "properties": {
                "level": {
                    "type": "string",
                    "description": "Log level: trace, debug, info, warn, error",
                    "enum": ["trace", "debug", "info", "warn", "error"]
                },
                "message": {
                    "type": "string",
                    "description": "Log message to emit"
                }
            },
            "required": ["level", "message"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            // Parse parameters
            let params: LogParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:log".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            // Parse log level
            let level =
                LogLevel::from_str(&params.level).map_err(|_| NikaError::BuiltinInvalidParams {
                    tool: "nika:log".into(),
                    reason: format!(
                        "Invalid log level '{}'. Valid levels: trace, debug, info, warn, error",
                        params.level
                    ),
                })?;

            // Emit log via tracing
            match level {
                LogLevel::Trace => trace!(target: "nika:log", "{}", params.message),
                LogLevel::Debug => debug!(target: "nika:log", "{}", params.message),
                LogLevel::Info => info!(target: "nika:log", "{}", params.message),
                LogLevel::Warn => warn!(target: "nika:log", "{}", params.message),
                LogLevel::Error => error!(target: "nika:log", "{}", params.message),
            }

            // Return response
            let response = LogResponse {
                logged: true,
                level: level.as_str().to_string(),
                message: params.message,
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:log".into(),
                reason: format!("Failed to serialize response: {}", e),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_tool_name() {
        let tool = LogTool;
        assert_eq!(tool.name(), "log");
    }

    #[test]
    fn test_log_tool_description() {
        let tool = LogTool;
        assert!(tool.description().contains("log"));
    }

    #[test]
    fn test_log_tool_schema() {
        let tool = LogTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["level"].is_object());
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("level")));
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("message")));
    }

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("trace"), Ok(LogLevel::Trace));
        assert_eq!(LogLevel::from_str("DEBUG"), Ok(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("Info"), Ok(LogLevel::Info));
        assert_eq!(LogLevel::from_str("warn"), Ok(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("WARNING"), Ok(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("error"), Ok(LogLevel::Error));
        assert_eq!(LogLevel::from_str("invalid"), Err(()));
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Trace.as_str(), "trace");
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Warn.as_str(), "warn");
        assert_eq!(LogLevel::Error.as_str(), "error");
    }

    #[tokio::test]
    async fn test_log_info() {
        let tool = LogTool;
        let result = tool
            .call(r#"{"level": "info", "message": "Test info message"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["logged"], true);
        assert_eq!(response["level"], "info");
        assert_eq!(response["message"], "Test info message");
    }

    #[tokio::test]
    async fn test_log_error() {
        let tool = LogTool;
        let result = tool
            .call(r#"{"level": "error", "message": "Test error message"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["logged"], true);
        assert_eq!(response["level"], "error");
    }

    #[tokio::test]
    async fn test_log_all_levels() {
        let tool = LogTool;

        for level in ["trace", "debug", "info", "warn", "error"] {
            let result = tool
                .call(format!(r#"{{"level": "{}", "message": "Test"}}"#, level))
                .await;
            assert!(result.is_ok(), "Failed for level: {}", level);
        }
    }

    #[tokio::test]
    async fn test_log_invalid_level() {
        let tool = LogTool;
        let result = tool
            .call(r#"{"level": "critical", "message": "Test"}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid log level"));
    }

    #[tokio::test]
    async fn test_log_invalid_json() {
        let tool = LogTool;
        let result = tool.call("not json".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_log_missing_level() {
        let tool = LogTool;
        let result = tool.call(r#"{"message": "Test"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_log_missing_message() {
        let tool = LogTool;
        let result = tool.call(r#"{"level": "info"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_log_case_insensitive_level() {
        let tool = LogTool;

        // Test uppercase
        let result = tool
            .call(r#"{"level": "INFO", "message": "Test"}"#.to_string())
            .await;
        assert!(result.is_ok());

        // Test mixed case
        let result = tool
            .call(r#"{"level": "WaRn", "message": "Test"}"#.to_string())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_log_warning_alias() {
        let tool = LogTool;
        let result = tool
            .call(r#"{"level": "warning", "message": "Test warning"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["level"], "warn"); // Normalized to "warn"
    }
}
