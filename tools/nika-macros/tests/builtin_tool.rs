// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for #[builtin_tool] attribute macro.

use nika_macros::builtin_tool;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

// ── Minimal BuiltinTool trait (mirrors the real one for test purposes) ──

pub trait BuiltinTool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str {
        ""
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>>;
}

// ── Minimal error type for tests ──

#[derive(Debug)]
pub struct BuiltinError {
    pub tool: String,
    pub reason: String,
}

impl std::fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.tool, self.reason)
    }
}

impl std::error::Error for BuiltinError {}

impl BuiltinError {
    pub fn invalid_params(tool: &str, reason: impl std::fmt::Display) -> Self {
        Self {
            tool: tool.to_string(),
            reason: format!("Invalid parameters: {}", reason),
        }
    }

    pub fn tool_error(tool: &str, reason: impl std::fmt::Display) -> Self {
        Self {
            tool: tool.to_string(),
            reason: format!("Tool error: {}", reason),
        }
    }
}

// ── Test tool ──

#[derive(Debug, Deserialize)]
struct EchoParams {
    message: String,
}

#[derive(Debug, Serialize)]
struct EchoResponse {
    echo: String,
}

#[builtin_tool(
    name = "echo",
    description = "Echo the message back",
    error = "BuiltinError",
    trait_path = "BuiltinTool",
)]
async fn echo_tool(params: EchoParams) -> Result<EchoResponse, BuiltinError> {
    Ok(EchoResponse {
        echo: params.message,
    })
}

#[test]
fn builtin_tool_name() {
    assert_eq!(EchoToolStub.name(), "echo");
}

#[test]
fn builtin_tool_description() {
    assert_eq!(EchoToolStub.description(), "Echo the message back");
}

#[tokio::test]
async fn builtin_tool_call_roundtrip() {
    let args = r#"{"message":"hello"}"#;
    let result = EchoToolStub.call(args.to_string()).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["echo"], "hello");
}

#[tokio::test]
async fn builtin_tool_invalid_json() {
    let result = EchoToolStub.call("not json".to_string()).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.reason.contains("Invalid parameters"));
}

#[tokio::test]
async fn builtin_tool_missing_field() {
    let result = EchoToolStub.call(r#"{}"#.to_string()).await;
    assert!(result.is_err());
}
