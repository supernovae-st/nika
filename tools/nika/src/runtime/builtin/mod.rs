//! Builtin Tools Module - nika:* tools for HITL and workflow composition (v0.9.3)
//!
//! Provides 6 builtin tools with nika: prefix:
//! - `nika:sleep` - Pause execution for duration
//! - `nika:log` - Emit log event at level
//! - `nika:emit` - Emit custom event to EventLog
//! - `nika:assert` - Validate condition, fail if false
//! - `nika:prompt` - HITL - request user input
//! - `nika:run` - Execute nested workflow
//!
//! # Architecture
//!
//! ```text
//! invoke: nika:sleep → BuiltinToolRouter → SleepTool.call()
//!                            │
//!                            ├── is_builtin("nika:*") = true
//!                            └── dispatch to appropriate tool
//! ```

mod assert;
mod emit;
mod log;
mod router;
mod sleep;
mod r#trait;

pub use assert::AssertTool;
pub use emit::EmitTool;
pub use log::{LogLevel, LogTool};
pub use router::BuiltinToolRouter;
pub use sleep::SleepTool;
pub use r#trait::BuiltinTool;

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
