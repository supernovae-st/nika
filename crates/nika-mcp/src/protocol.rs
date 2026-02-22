//! JSON-RPC 2.0 Protocol Types for MCP
//!
//! This module provides the core JSON-RPC 2.0 types used for MCP communication:
//! - [`JsonRpcRequest`]: Outgoing request to MCP server
//! - [`JsonRpcResponse`]: Incoming response from MCP server
//! - [`JsonRpcError`]: Error object in failed responses
//!
//! ## Protocol Overview
//!
//! MCP uses JSON-RPC 2.0 over stdio. Each message is a JSON object:
//!
//! ```json
//! // Request
//! {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {...}}
//!
//! // Success Response
//! {"jsonrpc": "2.0", "id": 1, "result": {...}}
//!
//! // Error Response
//! {"jsonrpc": "2.0", "id": 1, "error": {"code": -32600, "message": "..."}}
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::mcp::{JsonRpcRequest, JsonRpcResponse};
//! use serde_json::json;
//!
//! // Create a request
//! let request = JsonRpcRequest::new(1, "tools/call", json!({
//!     "name": "novanet_generate",
//!     "arguments": {"entity": "qr-code"}
//! }));
//!
//! // Serialize and send
//! let json = serde_json::to_string(&request)?;
//!
//! // Parse response
//! let response: JsonRpcResponse = serde_json::from_str(&response_str)?;
//! if response.is_success() {
//!     let result = response.result.unwrap();
//! }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 Request.
///
/// Sent to an MCP server to invoke a method (e.g., `initialize`, `tools/call`).
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    /// Protocol version - always "2.0"
    pub jsonrpc: &'static str,

    /// Request ID - used to correlate responses
    pub id: u64,

    /// Method name (e.g., "initialize", "tools/call", "resources/read")
    pub method: String,

    /// Method parameters
    pub params: Value,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique request ID for response correlation
    /// * `method` - Method name to invoke
    /// * `params` - Method parameters as JSON value
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let request = JsonRpcRequest::new(1, "tools/list", json!({}));
    /// ```
    pub fn new(id: u64, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        }
    }
}

/// JSON-RPC 2.0 Notification.
///
/// A notification is a request without an ID - the server should not respond.
/// Used for one-way messages like `notifications/initialized`.
#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    /// Protocol version - always "2.0"
    pub jsonrpc: &'static str,

    /// Method name (e.g., "notifications/initialized")
    pub method: String,

    /// Method parameters (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcNotification {
    /// Create a new JSON-RPC notification.
    ///
    /// # Arguments
    ///
    /// * `method` - Notification method name
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let notification = JsonRpcNotification::new("notifications/initialized");
    /// ```
    pub fn new(method: &str) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.to_string(),
            params: None,
        }
    }

    /// Create a notification with parameters.
    pub fn with_params(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.to_string(),
            params: Some(params),
        }
    }
}

/// JSON-RPC 2.0 Response.
///
/// Received from an MCP server after a request. Contains either a result or an error.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    /// Protocol version - should be "2.0"
    pub jsonrpc: String,

    /// Request ID this response corresponds to (null for notifications)
    #[serde(default)]
    pub id: Option<u64>,

    /// Successful result (mutually exclusive with error)
    #[serde(default)]
    pub result: Option<Value>,

    /// Error information (mutually exclusive with result)
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Check if the response indicates success.
    ///
    /// A response is successful if it has a result and no error.
    /// Note: a null result is still considered success.
    pub fn is_success(&self) -> bool {
        self.result.is_some() && self.error.is_none()
    }
}

/// JSON-RPC 2.0 Error object.
///
/// Returned in the `error` field of a response when the request fails.
///
/// ## Standard Error Codes
///
/// | Code | Message |
/// |------|---------|
/// | -32700 | Parse error |
/// | -32600 | Invalid Request |
/// | -32601 | Method not found |
/// | -32602 | Invalid params |
/// | -32603 | Internal error |
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    /// Error code (negative integer per JSON-RPC spec)
    pub code: i32,

    /// Human-readable error message
    pub message: String,

    /// Additional error data (optional, implementation-defined)
    #[serde(default)]
    pub data: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_new() {
        let request = JsonRpcRequest::new(1, "test", json!({}));

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, 1);
        assert_eq!(request.method, "test");
    }

    #[test]
    fn test_response_is_success() {
        let json_str = r#"{"jsonrpc": "2.0", "id": 1, "result": {}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();

        assert!(response.is_success());
    }

    #[test]
    fn test_response_is_not_success_on_error() {
        let json_str = r#"{"jsonrpc": "2.0", "id": 1, "error": {"code": -1, "message": "fail"}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();

        assert!(!response.is_success());
    }

    #[test]
    fn test_notification_new() {
        let notification = JsonRpcNotification::new("notifications/initialized");

        assert_eq!(notification.jsonrpc, "2.0");
        assert_eq!(notification.method, "notifications/initialized");
        assert!(notification.params.is_none());
    }

    #[test]
    fn test_notification_with_params() {
        let notification =
            JsonRpcNotification::with_params("notifications/test", json!({"key": "value"}));

        assert_eq!(notification.jsonrpc, "2.0");
        assert_eq!(notification.method, "notifications/test");
        assert!(notification.params.is_some());
    }

    #[test]
    fn test_notification_serializes_without_params() {
        let notification = JsonRpcNotification::new("notifications/initialized");
        let json = serde_json::to_string(&notification).unwrap();

        // Should not include "params" field when None
        assert!(!json.contains("params"));
        assert!(json.contains("notifications/initialized"));
    }
}
