//! Integration tests for MCP JSON-RPC 2.0 Protocol Types
//!
//! Tests serialization, deserialization, and helper methods for JSON-RPC types.

use nika::mcp::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use serde_json::json;

// ═══════════════════════════════════════════════════════════════
// JSON-RPC REQUEST TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_json_rpc_request_new_creates_valid_request() {
    let request = JsonRpcRequest::new(1, "tools/call", json!({"name": "test"}));

    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.id, 1);
    assert_eq!(request.method, "tools/call");
    assert_eq!(request.params, json!({"name": "test"}));
}

#[test]
fn test_json_rpc_request_serialization() {
    let request = JsonRpcRequest::new(42, "initialize", json!({}));

    let json_str = serde_json::to_string(&request).expect("serialization should succeed");

    // Parse back to verify structure
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["jsonrpc"], "2.0");
    assert_eq!(parsed["id"], 42);
    assert_eq!(parsed["method"], "initialize");
}

#[test]
fn test_json_rpc_request_serialization_with_complex_params() {
    let params = json!({
        "name": "novanet_generate",
        "arguments": {
            "entity": "qr-code",
            "locale": "fr-FR",
            "forms": ["title", "description"]
        }
    });
    let request = JsonRpcRequest::new(1, "tools/call", params.clone());

    let json_str = serde_json::to_string(&request).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["params"], params);
}

#[test]
fn test_json_rpc_request_with_null_params() {
    let request = JsonRpcRequest::new(1, "ping", serde_json::Value::Null);

    let json_str = serde_json::to_string(&request).unwrap();

    assert!(json_str.contains("\"params\":null") || json_str.contains("\"params\": null"));
}

// ═══════════════════════════════════════════════════════════════
// JSON-RPC RESPONSE SUCCESS TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_json_rpc_response_success_parse() {
    let json_str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {"status": "ok", "data": [1, 2, 3]}
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).expect("parse should succeed");

    assert_eq!(response.jsonrpc, "2.0");
    assert_eq!(response.id, Some(1));
    assert!(response.result.is_some());
    assert!(response.error.is_none());
    assert!(response.is_success());
}

#[test]
fn test_json_rpc_response_success_extracts_result() {
    let json_str = r#"{
        "jsonrpc": "2.0",
        "id": 42,
        "result": {"tools": [{"name": "read_file"}, {"name": "write_file"}]}
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();

    let result = response.result.unwrap();
    assert!(result["tools"].is_array());
    assert_eq!(result["tools"].as_array().unwrap().len(), 2);
}

// ═══════════════════════════════════════════════════════════════
// JSON-RPC RESPONSE ERROR TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_json_rpc_response_error_parse() {
    let json_str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "error": {
            "code": -32600,
            "message": "Invalid Request"
        }
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).expect("parse should succeed");

    assert_eq!(response.jsonrpc, "2.0");
    assert_eq!(response.id, Some(1));
    assert!(response.result.is_none());
    assert!(response.error.is_some());
    assert!(!response.is_success());
}

#[test]
fn test_json_rpc_response_error_with_data() {
    let json_str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "error": {
            "code": -32602,
            "message": "Invalid params",
            "data": {"field": "entity", "reason": "required"}
        }
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();
    let error = response.error.unwrap();

    assert_eq!(error.code, -32602);
    assert_eq!(error.message, "Invalid params");
    assert!(error.data.is_some());
    assert_eq!(error.data.as_ref().unwrap()["field"], "entity");
}

#[test]
fn test_json_rpc_response_is_success_returns_false_for_error() {
    let json_str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "error": {"code": -32600, "message": "Error"}
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();

    assert!(!response.is_success());
}

#[test]
fn test_json_rpc_response_is_success_returns_true_for_empty_result() {
    let json_str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {}
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();

    // Empty object result is a success
    assert!(response.is_success());
}

#[test]
fn test_json_rpc_response_null_result_is_not_success() {
    // Note: JSON-RPC 2.0 spec says result should be omitted on error,
    // but when result is explicitly null, serde deserializes it as None
    // This is treated as "no result" which is not a success
    let json_str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": null
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();

    // null is deserialized as None, so is_success() returns false
    // This is acceptable behavior - MCP servers return {} not null
    assert!(!response.is_success());
}

// ═══════════════════════════════════════════════════════════════
// JSON-RPC ERROR TYPE TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_json_rpc_error_minimal() {
    let json_str = r#"{
        "code": -32700,
        "message": "Parse error"
    }"#;

    let error: JsonRpcError = serde_json::from_str(json_str).expect("parse should succeed");

    assert_eq!(error.code, -32700);
    assert_eq!(error.message, "Parse error");
    assert!(error.data.is_none());
}

#[test]
fn test_json_rpc_error_standard_codes() {
    // Test standard JSON-RPC error codes
    let test_cases = vec![
        (-32700, "Parse error"),
        (-32600, "Invalid Request"),
        (-32601, "Method not found"),
        (-32602, "Invalid params"),
        (-32603, "Internal error"),
    ];

    for (code, message) in test_cases {
        let json_str = format!(r#"{{"code": {}, "message": "{}"}}"#, code, message);
        let error: JsonRpcError = serde_json::from_str(&json_str).unwrap();

        assert_eq!(error.code, code);
        assert_eq!(error.message, message);
    }
}

// ═══════════════════════════════════════════════════════════════
// EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_json_rpc_response_with_null_id() {
    // Notifications can have null id
    let json_str = r#"{
        "jsonrpc": "2.0",
        "id": null,
        "result": {}
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();

    assert!(response.id.is_none());
}

#[test]
fn test_json_rpc_response_without_id_field() {
    // Some servers omit id entirely for notifications
    let json_str = r#"{
        "jsonrpc": "2.0",
        "result": {"notification": true}
    }"#;

    let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();

    assert!(response.id.is_none());
    assert!(response.is_success());
}

// ═══════════════════════════════════════════════════════════════
// DEBUG TRAIT TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_json_rpc_request_implements_debug() {
    let request = JsonRpcRequest::new(1, "test", json!({}));
    let debug_output = format!("{:?}", request);

    assert!(debug_output.contains("JsonRpcRequest"));
    assert!(debug_output.contains("test"));
}

#[test]
fn test_json_rpc_response_implements_debug() {
    let json_str = r#"{"jsonrpc": "2.0", "id": 1, "result": {}}"#;
    let response: JsonRpcResponse = serde_json::from_str(json_str).unwrap();
    let debug_output = format!("{:?}", response);

    assert!(debug_output.contains("JsonRpcResponse"));
}

#[test]
fn test_json_rpc_error_implements_debug() {
    let json_str = r#"{"code": -32600, "message": "Test"}"#;
    let error: JsonRpcError = serde_json::from_str(json_str).unwrap();
    let debug_output = format!("{:?}", error);

    assert!(debug_output.contains("JsonRpcError"));
}
