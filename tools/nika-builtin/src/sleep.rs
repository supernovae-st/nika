// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:sleep - Pause execution for duration.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "duration": "1s"  // humantime format: 1s, 500ms, 1m30s, etc.
//! }
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "slept_for_ms": 1000
//! }
//! ```
//!
//! # Limits
//!
//! Maximum sleep duration is 5 minutes (300 seconds) to prevent workflow blocking.
//! Attempts to sleep longer will return an error.

use crate::{BuiltinError, BuiltinTool, __sealed};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Maximum allowed sleep duration (5 minutes)
///
/// This limit prevents agents from blocking workflows indefinitely by calling
/// nika:sleep with extremely long durations like "1000h".
pub const MAX_SLEEP_DURATION: Duration = Duration::from_secs(5 * 60);

/// Parameters for nika:sleep tool.
#[derive(Debug, Clone, Deserialize)]
struct SleepParams {
    /// Duration string in humantime format (e.g., "1s", "500ms", "1m30s").
    duration: String,
}

/// Response from nika:sleep tool.
#[derive(Debug, Clone, Serialize)]
struct SleepResponse {
    /// Actual duration slept in milliseconds.
    slept_for_ms: u64,
}

/// nika:sleep builtin tool.
///
/// Pauses execution for the specified duration.
pub struct SleepTool;

impl __sealed::Sealed for SleepTool {}

impl BuiltinTool for SleepTool {
    fn name(&self) -> &'static str {
        "sleep"
    }

    fn description(&self) -> &'static str {
        "Pause execution for the specified duration"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "duration": {
                    "type": "string",
                    "description": "Duration to sleep in humantime format (e.g., '1s', '500ms', '1m30s')"
                }
            },
            "required": ["duration"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: SleepParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::InvalidArgs {
                    tool: "nika:sleep".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            let duration = humantime::parse_duration(&params.duration).map_err(|e| {
                BuiltinError::InvalidArgs {
                    tool: "nika:sleep".into(),
                    reason: format!("Invalid duration '{}': {}", params.duration, e),
                }
            })?;

            if duration > MAX_SLEEP_DURATION {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:sleep".into(),
                    reason: format!(
                        "Sleep duration '{}' exceeds maximum allowed {} seconds. \
                         This limit prevents workflows from being blocked indefinitely.",
                        params.duration,
                        MAX_SLEEP_DURATION.as_secs()
                    ),
                });
            }

            tokio::time::sleep(duration).await;

            let response = SleepResponse {
                slept_for_ms: duration.as_millis() as u64,
            };

            serde_json::to_string(&response).map_err(|e| BuiltinError::Other {
                tool: "nika:sleep".into(),
                reason: format!("Failed to serialize response: {}", e),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sleep_tool_name() {
        let tool = SleepTool;
        assert_eq!(tool.name(), "sleep");
    }

    #[test]
    fn test_sleep_tool_description() {
        let tool = SleepTool;
        assert!(tool.description().contains("Pause"));
    }

    #[test]
    fn test_sleep_tool_schema() {
        let tool = SleepTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["duration"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("duration")));
    }

    #[tokio::test]
    async fn test_sleep_executes() {
        let tool = SleepTool;
        let start = std::time::Instant::now();

        let result = tool.call(r#"{"duration": "10ms"}"#.to_string()).await;

        assert!(result.is_ok());
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() >= 10);

        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["slept_for_ms"], 10);
    }

    #[tokio::test]
    async fn test_sleep_parses_seconds() {
        let tool = SleepTool;
        let result = tool.call(r#"{"duration": "1ms"}"#.to_string()).await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["slept_for_ms"], 1);
    }

    #[tokio::test]
    async fn test_sleep_parses_complex_duration() {
        let duration = humantime::parse_duration("1s500ms");
        assert!(duration.is_ok());
        assert_eq!(duration.unwrap().as_millis(), 1500);
    }

    #[tokio::test]
    async fn test_sleep_invalid_duration() {
        let tool = SleepTool;
        let result = tool
            .call(r#"{"duration": "not-a-duration"}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid duration"));
    }

    #[tokio::test]
    async fn test_sleep_invalid_json() {
        let tool = SleepTool;
        let result = tool.call("not json".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_sleep_missing_duration() {
        let tool = SleepTool;
        let result = tool.call(r#"{}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[test]
    fn test_max_sleep_duration_is_5_minutes() {
        assert_eq!(MAX_SLEEP_DURATION.as_secs(), 300);
        assert_eq!(MAX_SLEEP_DURATION, std::time::Duration::from_secs(5 * 60));
    }

    #[tokio::test]
    async fn test_sleep_rejects_excessive_duration_hours() {
        let tool = SleepTool;
        let result = tool.call(r#"{"duration": "1000h"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("exceeds maximum"));
        assert!(err_str.contains("300"));
    }

    #[tokio::test]
    async fn test_sleep_rejects_6_minutes() {
        let tool = SleepTool;
        let result = tool.call(r#"{"duration": "6m"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn test_sleep_accepts_5_minutes() {
        let _tool = SleepTool;
        let duration = humantime::parse_duration("5m").unwrap();
        assert!(duration <= MAX_SLEEP_DURATION);
    }

    #[tokio::test]
    async fn test_sleep_accepts_4_minutes() {
        let _tool = SleepTool;
        let duration = humantime::parse_duration("4m").unwrap();
        assert!(duration < MAX_SLEEP_DURATION);
    }

    #[tokio::test]
    async fn test_sleep_accepts_just_under_limit() {
        let _tool = SleepTool;
        let duration = humantime::parse_duration("299s").unwrap();
        assert!(duration < MAX_SLEEP_DURATION);
    }

    #[tokio::test]
    async fn test_sleep_rejects_just_over_limit() {
        let tool = SleepTool;
        let result = tool.call(r#"{"duration": "301s"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeds maximum"));
    }
}
