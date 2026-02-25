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

use super::BuiltinTool;
use crate::error::NikaError;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

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
            "required": ["duration"]
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            // Parse parameters
            let params: SleepParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:sleep".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            // Parse duration using humantime
            let duration = humantime::parse_duration(&params.duration).map_err(|e| {
                NikaError::BuiltinInvalidParams {
                    tool: "nika:sleep".into(),
                    reason: format!("Invalid duration '{}': {}", params.duration, e),
                }
            })?;

            // Sleep
            tokio::time::sleep(duration).await;

            // Return response
            let response = SleepResponse {
                slept_for_ms: duration.as_millis() as u64,
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
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
        // Should have slept at least 10ms (allow some tolerance)
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
        // Test humantime's ability to parse complex durations
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
}
