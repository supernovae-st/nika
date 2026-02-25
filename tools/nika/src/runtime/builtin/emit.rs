//! nika:emit - Emit custom event to EventLog.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "name": "my_event",      // Event name/type
//!   "payload": { ... }       // Arbitrary JSON payload (optional)
//! }
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "emitted": true,
//!   "name": "my_event",
//!   "payload": { ... }
//! }
//! ```

use super::BuiltinTool;
use crate::error::NikaError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// Parameters for nika:emit tool.
#[derive(Debug, Clone, Deserialize)]
struct EmitParams {
    /// Event name/type.
    name: String,
    /// Event payload (arbitrary JSON, optional).
    #[serde(default)]
    payload: Value,
}

/// Response from nika:emit tool.
#[derive(Debug, Clone, Serialize)]
struct EmitResponse {
    /// Whether the event was emitted.
    emitted: bool,
    /// The event name.
    name: String,
    /// The payload that was emitted.
    payload: Value,
}

/// nika:emit builtin tool.
///
/// Emits custom events to the EventLog for workflow observability.
/// Events can carry arbitrary JSON payloads.
pub struct EmitTool;

impl BuiltinTool for EmitTool {
    fn name(&self) -> &'static str {
        "emit"
    }

    fn description(&self) -> &'static str {
        "Emit custom event to EventLog for workflow observability"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Event name/type identifier"
                },
                "payload": {
                    "type": "object",
                    "description": "Arbitrary JSON payload for the event"
                }
            },
            "required": ["name"]
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            // Parse parameters
            let params: EmitParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:emit".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            // Validate event name
            if params.name.is_empty() {
                return Err(NikaError::BuiltinInvalidParams {
                    tool: "nika:emit".into(),
                    reason: "Event name cannot be empty".into(),
                });
            }

            // Note: The actual EventLog emission will be handled by the Router
            // when integrated with the Executor. For now, we just validate and
            // return success. The Router will capture the params and emit the
            // EventKind::Custom event.
            tracing::debug!(
                target: "nika:emit",
                name = %params.name,
                "Custom event emitted"
            );

            // Return response
            let response = EmitResponse {
                emitted: true,
                name: params.name,
                payload: params.payload,
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:emit".into(),
                reason: format!("Failed to serialize response: {}", e),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_tool_name() {
        let tool = EmitTool;
        assert_eq!(tool.name(), "emit");
    }

    #[test]
    fn test_emit_tool_description() {
        let tool = EmitTool;
        assert!(tool.description().contains("EventLog"));
    }

    #[test]
    fn test_emit_tool_schema() {
        let tool = EmitTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["name"].is_object());
        assert!(schema["properties"]["payload"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("name")));
    }

    #[tokio::test]
    async fn test_emit_simple_event() {
        let tool = EmitTool;
        let result = tool
            .call(r#"{"name": "test_event"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["emitted"], true);
        assert_eq!(response["name"], "test_event");
        assert_eq!(response["payload"], serde_json::json!(null));
    }

    #[tokio::test]
    async fn test_emit_with_payload() {
        let tool = EmitTool;
        let result = tool
            .call(
                r#"{"name": "user_action", "payload": {"action": "click", "target": "button"}}"#
                    .to_string(),
            )
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["emitted"], true);
        assert_eq!(response["name"], "user_action");
        assert_eq!(response["payload"]["action"], "click");
        assert_eq!(response["payload"]["target"], "button");
    }

    #[tokio::test]
    async fn test_emit_with_complex_payload() {
        let tool = EmitTool;
        let result = tool
            .call(
                r#"{
                    "name": "metrics",
                    "payload": {
                        "count": 42,
                        "values": [1, 2, 3],
                        "nested": {"key": "value"}
                    }
                }"#
                .to_string(),
            )
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["payload"]["count"], 42);
        assert_eq!(response["payload"]["values"][1], 2);
        assert_eq!(response["payload"]["nested"]["key"], "value");
    }

    #[tokio::test]
    async fn test_emit_empty_name() {
        let tool = EmitTool;
        let result = tool
            .call(r#"{"name": ""}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_emit_missing_name() {
        let tool = EmitTool;
        let result = tool
            .call(r#"{"payload": {"test": 1}}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_emit_invalid_json() {
        let tool = EmitTool;
        let result = tool.call("not json".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_emit_null_payload() {
        let tool = EmitTool;
        let result = tool
            .call(r#"{"name": "event", "payload": null}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["payload"], serde_json::json!(null));
    }

    #[tokio::test]
    async fn test_emit_array_payload() {
        let tool = EmitTool;
        let result = tool
            .call(r#"{"name": "items", "payload": [1, 2, 3]}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["payload"][0], 1);
        assert_eq!(response["payload"][2], 3);
    }

    #[tokio::test]
    async fn test_emit_string_payload() {
        let tool = EmitTool;
        let result = tool
            .call(r#"{"name": "message", "payload": "hello world"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["payload"], "hello world");
    }
}
