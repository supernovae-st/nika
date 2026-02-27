//! Builtin Tools Module - nika_* tools for HITL and workflow composition (v0.12.1)
//!
//! Provides 6 builtin tools with nika_ prefix:
//! - `nika_sleep` - Pause execution for duration
//! - `nika_log` - Emit log event at level
//! - `nika_emit` - Emit custom event to EventLog
//! - `nika_assert` - Validate condition, fail if false
//! - `nika_prompt` - HITL - request user input
//! - `nika_run` - Execute nested workflow
//!
//! v0.12.1: Changed prefix from `nika:` to `nika_` for Anthropic API compatibility.
//! Tool name pattern: ^[a-zA-Z0-9_-]{1,128}$ - colon is NOT allowed.
//!
//! # Architecture
//!
//! ```text
//! invoke: nika_sleep → BuiltinToolRouter → SleepTool.call()
//!                            │
//!                            ├── is_builtin("nika_*") = true
//!                            └── dispatch to appropriate tool
//! ```

mod assert;
mod emit;
mod log;
mod prompt;
mod rig_adapter;
mod router;
mod run;
mod sleep;
mod r#trait;

pub use assert::AssertTool;
pub use emit::EmitTool;
pub use log::{LogLevel, LogTool};
pub use prompt::{PromptParams, PromptResponse, PromptTool};
pub use r#trait::BuiltinTool;
pub use rig_adapter::NikaBuiltinToolAdapter;
pub use router::BuiltinToolRouter;
pub use run::{RunParams, RunResponse, RunTool};
pub use sleep::SleepTool;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NikaError;

    #[test]
    fn test_builtin_tool_trait_exists() {
        struct TestTool;

        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }

            fn call<'a>(
                &'a self,
                _args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
            > {
                Box::pin(async { Ok("ok".to_string()) })
            }
        }

        let tool = TestTool;
        assert_eq!(tool.name(), "test");
    }

    #[test]
    fn test_builtin_tool_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        struct TestTool;

        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }

            fn call<'a>(
                &'a self,
                _args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
            > {
                Box::pin(async { Ok("ok".to_string()) })
            }
        }

        assert_send_sync::<TestTool>();
    }

    #[test]
    fn test_builtin_tool_default_description() {
        struct TestTool;

        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }

            fn call<'a>(
                &'a self,
                _args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
            > {
                Box::pin(async { Ok("ok".to_string()) })
            }
        }

        let tool = TestTool;
        assert_eq!(tool.description(), "");
    }

    #[test]
    fn test_builtin_tool_default_parameters_schema() {
        struct TestTool;

        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }

            fn call<'a>(
                &'a self,
                _args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
            > {
                Box::pin(async { Ok("ok".to_string()) })
            }
        }

        let tool = TestTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema, serde_json::json!({}));
    }
}
