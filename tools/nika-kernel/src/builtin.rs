// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! BuiltinTool trait and BuiltinError for nika:* tools.
//!
//! This module defines the contract that all 63 builtin tools implement.
//! The trait is **sealed** — only types that implement `__sealed::Sealed`
//! can implement `BuiltinTool`. Use the `#[builtin_tool]` macro from
//! `nika-macros` or manually impl `Sealed` in downstream crates.
//!
//! # Error type
//!
//! Tools return `Result<String, BuiltinError>` instead of `NikaError`.
//! The engine converts via `impl From<BuiltinError> for NikaError`.

use std::future::Future;
use std::pin::Pin;

/// Sealed trait module — prevents external crates from implementing `BuiltinTool`.
///
/// This is `#[doc(hidden)]` by convention: downstream crates (nika-builtin,
/// nika-engine) can implement `Sealed`, but arbitrary third-party crates should not.
#[doc(hidden)]
pub mod __sealed {
    /// Marker trait for sealed `BuiltinTool` implementations.
    pub trait Sealed {}
}

/// Trait for builtin nika:* tools.
///
/// All builtin tools must be Send + Sync to support concurrent execution
/// in the DAG runner. The trait is sealed via `__sealed::Sealed`.
///
/// # Example
///
/// ```ignore
/// use nika_kernel::builtin::{BuiltinTool, BuiltinError, __sealed::Sealed};
///
/// pub struct SleepTool;
/// impl Sealed for SleepTool {}
/// impl BuiltinTool for SleepTool {
///     fn name(&self) -> &'static str { "sleep" }
///     fn call<'a>(&'a self, args: String) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
///         Box::pin(async move { Ok("{}".to_string()) })
///     }
/// }
/// ```
pub trait BuiltinTool: __sealed::Sealed + Send + Sync {
    /// Tool name (without nika: prefix).
    ///
    /// # Example
    /// ```ignore
    /// fn name(&self) -> &'static str { "sleep" }  // for nika:sleep
    /// ```
    fn name(&self) -> &'static str;

    /// Tool description for LLM tool discovery.
    ///
    /// Defaults to empty string if not overridden.
    fn description(&self) -> &'static str {
        ""
    }

    /// JSON schema for tool parameters.
    ///
    /// Used for tool discovery and validation.
    /// Defaults to empty object if not overridden.
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    /// Execute the tool with JSON-encoded arguments.
    ///
    /// Returns the tool result as a JSON string.
    ///
    /// # Arguments
    /// * `args` - JSON-encoded parameters for the tool
    ///
    /// # Returns
    /// * `Ok(String)` - JSON-encoded result on success
    /// * `Err(BuiltinError)` - Tool execution error
    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>>;
}

// ─────────────────────────────────────────────────────────────────────
// BuiltinError
// ─────────────────────────────────────────────────────────────────────

/// Error type for builtin tools.
///
/// Maps to NIKA-210 (BuiltinToolError), NIKA-212 (BuiltinInvalidParams),
/// and NIKA-213 (AssertionFailed) in the engine's `NikaError`.
///
/// The engine provides `impl From<BuiltinError> for NikaError`.
#[derive(Debug, thiserror::Error)]
pub enum BuiltinError {
    /// Invalid tool parameters (NIKA-212).
    #[error("[NIKA-212] Builtin tool '{tool}' invalid parameters: {reason}")]
    InvalidArgs { tool: String, reason: String },

    /// I/O error during tool execution (NIKA-210).
    #[error("[NIKA-210] Builtin tool '{tool}' error: {reason}")]
    Io { tool: String, reason: String },

    /// Parse error (NIKA-210).
    #[error("[NIKA-210] Builtin tool '{tool}' parse error: {reason}")]
    Parse { tool: String, reason: String },

    /// Tool execution timed out (NIKA-210).
    #[error("[NIKA-210] Builtin tool '{tool}' timed out")]
    Timeout { tool: String },

    /// JSON schema validation error (NIKA-210).
    #[error("[NIKA-210] Builtin tool '{tool}' schema error: {reason}")]
    Schema { tool: String, reason: String },

    /// Security denial — Shield blocked the operation (NIKA-380).
    #[error("[NIKA-380] Builtin tool '{tool}' denied: {reason}")]
    Denied { tool: String, reason: String },

    /// Assertion failed — nika:assert condition was false (NIKA-213).
    #[error("[NIKA-213] Assertion failed in nika:assert: {message}")]
    AssertionFailed { message: String, condition: String },

    /// Generic tool error (NIKA-210).
    #[error("[NIKA-210] Builtin tool '{tool}' error: {reason}")]
    Other { tool: String, reason: String },
}

impl BuiltinError {
    /// Create an `InvalidArgs` error.
    ///
    /// This is the constructor the `#[builtin_tool]` macro calls for
    /// parameter deserialization failures.
    pub fn invalid_params(tool: impl Into<String>, reason: impl std::fmt::Display) -> Self {
        Self::InvalidArgs {
            tool: tool.into(),
            reason: reason.to_string(),
        }
    }

    /// Create an `Other` error.
    ///
    /// This is the constructor the `#[builtin_tool]` macro calls for
    /// response serialization failures.
    pub fn tool_error(tool: impl Into<String>, reason: impl std::fmt::Display) -> Self {
        Self::Other {
            tool: tool.into(),
            reason: reason.to_string(),
        }
    }

    /// Create a `Denied` error (NIKA-380 — Shield capability denial).
    ///
    /// S12.P1 — replaces the repeated `BuiltinError::Denied { tool: ...,
    /// reason: ... }` struct construction across the 4 Shield-guarded file
    /// tools. The `Display` impl prefixes `[NIKA-380]` automatically.
    pub fn denied(tool: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Denied {
            tool: tool.into(),
            reason: reason.into(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // ── BuiltinError Display ──

    #[test]
    fn test_builtin_error_invalid_args_display() {
        let err = BuiltinError::InvalidArgs {
            tool: "nika:sleep".into(),
            reason: "missing duration".into(),
        };
        assert!(err.to_string().contains("NIKA-212"));
        assert!(err.to_string().contains("nika:sleep"));
        assert!(err.to_string().contains("missing duration"));
    }

    #[test]
    fn test_builtin_error_io_display() {
        let err = BuiltinError::Io {
            tool: "nika:read".into(),
            reason: "file not found".into(),
        };
        assert!(err.to_string().contains("NIKA-210"));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_builtin_error_timeout_display() {
        let err = BuiltinError::Timeout {
            tool: "nika:fetch".into(),
        };
        assert!(err.to_string().contains("NIKA-210"));
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn test_builtin_error_denied_display() {
        let err = BuiltinError::Denied {
            tool: "nika:read".into(),
            reason: "untrusted agent".into(),
        };
        assert!(err.to_string().contains("NIKA-380"));
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn test_builtin_error_assertion_failed_display() {
        let err = BuiltinError::AssertionFailed {
            message: "expected X".into(),
            condition: "false".into(),
        };
        assert!(err.to_string().contains("NIKA-213"));
        assert!(err.to_string().contains("expected X"));
    }

    #[test]
    fn test_builtin_error_other_display() {
        let err = BuiltinError::Other {
            tool: "nika:emit".into(),
            reason: "serialization failed".into(),
        };
        assert!(err.to_string().contains("NIKA-210"));
        assert!(err.to_string().contains("serialization failed"));
    }

    // ── Constructor methods (macro compat) ──

    #[test]
    fn test_invalid_params_constructor() {
        let err = BuiltinError::invalid_params("nika:sleep", "bad duration");
        match &err {
            BuiltinError::InvalidArgs { tool, reason } => {
                assert_eq!(tool, "nika:sleep");
                assert_eq!(reason, "bad duration");
            }
            other => panic!("expected InvalidArgs, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_error_constructor() {
        let err = BuiltinError::tool_error("nika:emit", "serialize failed");
        match &err {
            BuiltinError::Other { tool, reason } => {
                assert_eq!(tool, "nika:emit");
                assert_eq!(reason, "serialize failed");
            }
            other => panic!("expected Other, got {:?}", other),
        }
    }

    #[test]
    fn test_denied_constructor() {
        // S12.P1 — `denied()` builds BuiltinError::Denied with [NIKA-380]
        // surfaced via the Display impl.
        let err = BuiltinError::denied("nika:write", "tainted agent cannot write");
        match &err {
            BuiltinError::Denied { tool, reason } => {
                assert_eq!(tool, "nika:write");
                assert_eq!(reason, "tainted agent cannot write");
            }
            other => panic!("expected Denied, got {:?}", other),
        }
        assert!(err.to_string().contains("NIKA-380"));
        assert!(err.to_string().contains("nika:write"));
    }

    // ── BuiltinTool trait: object safety + Send + Sync ──

    /// Compile-time proof that `dyn BuiltinTool` is object-safe
    /// and `Arc<dyn BuiltinTool>` is Send + Sync.
    #[test]
    fn test_builtin_tool_object_safe_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<dyn BuiltinTool>>();
    }

    /// Verify a concrete implementation compiles through the sealed trait.
    #[test]
    fn test_builtin_tool_concrete_impl() {
        struct TestTool;
        impl __sealed::Sealed for TestTool {}
        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }
            fn call<'a>(
                &'a self,
                _args: String,
            ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
                Box::pin(async { Ok(r#"{"ok":true}"#.to_string()) })
            }
        }

        let tool = TestTool;
        assert_eq!(tool.name(), "test");
        assert_eq!(tool.description(), "");
        assert_eq!(tool.parameters_schema(), serde_json::json!({}));
    }

    /// Verify the default description and schema methods.
    #[test]
    fn test_builtin_tool_defaults() {
        struct MinimalTool;
        impl __sealed::Sealed for MinimalTool {}
        impl BuiltinTool for MinimalTool {
            fn name(&self) -> &'static str {
                "minimal"
            }
            fn call<'a>(
                &'a self,
                _args: String,
            ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
                Box::pin(async { Ok("{}".to_string()) })
            }
        }

        let tool = MinimalTool;
        assert_eq!(tool.description(), "");
        assert_eq!(tool.parameters_schema(), serde_json::json!({}));
    }

    /// Verify BuiltinError is Send + Sync (required for async boundaries).
    #[test]
    fn test_builtin_error_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BuiltinError>();
    }

    // ── Schema + Parse variants ──

    #[test]
    fn test_builtin_error_schema_display() {
        let err = BuiltinError::Schema {
            tool: "nika:jq".into(),
            reason: "invalid expression".into(),
        };
        assert!(err.to_string().contains("NIKA-210"));
        assert!(err.to_string().contains("schema error"));
    }

    #[test]
    fn test_builtin_error_parse_display() {
        let err = BuiltinError::Parse {
            tool: "nika:json_merge".into(),
            reason: "invalid JSON".into(),
        };
        assert!(err.to_string().contains("NIKA-210"));
        assert!(err.to_string().contains("parse error"));
    }
}
