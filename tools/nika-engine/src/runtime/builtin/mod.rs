//! Builtin Tools Module - nika:* tools for HITL, workflow composition, file operations, and completion
//!
//! Provides 12 builtin tools with nika: prefix (7 core + 5 file):
//!
//! **Core tools (7):**
//! - `nika:sleep` - Pause execution for duration
//! - `nika:log` - Emit log event at level
//! - `nika:emit` - Emit custom event to EventLog
//! - `nika:assert` - Validate condition, fail if false
//! - `nika:prompt` - HITL - request user input
//! - `nika:run` - Execute nested workflow
//! - `nika:complete` - Signal agent task completion
//!
//! **File tools (5):**
//! - `nika:read` - Read file with line numbers
//! - `nika:write` - Create/overwrite file
//! - `nika:edit` - Modify file (old_string → new_string)
//! - `nika:glob` - Find files by pattern
//! - `nika:grep` - Search content with regex
//!
//! # Architecture
//!
//! ```text
//! invoke: nika_sleep → BuiltinToolRouter → SleepTool.call()
//!                            │
//!                            ├── is_builtin("nika_*") = true
//!                            └── dispatch to appropriate tool
//! ```

mod aggregate;
mod assert;
mod complete;
mod cost;
mod data_tools;
mod emit;
mod fetch_tool;
mod file_adapter;
mod introspect_dag;
mod introspect_orchestrate;
mod introspect_task;
mod introspect_threads;
mod json_transform;
mod json_verify;
mod locale_lookup;
mod log;
pub(crate) mod media;
mod prompt;
mod records;
mod rig_adapter;
mod router;
mod run;
mod sleep;
mod r#trait;
mod yaml_validate;

pub use aggregate::AggregateTool;
pub use assert::AssertTool;
pub use complete::{
    is_completion_signal, parse_completion_response, CompleteParams, CompleteResponse,
    CompleteTool, COMPLETION_MARKER,
};
pub use cost::CostTool;
pub use data_tools::{
    ChunkTool, FilterTool, GroupByTool, JsonMergeTool, JsonQueryTool, MapTool, SetDiffTool,
    TokenCountTool, ZipTool,
};
pub use emit::EmitTool;
pub use json_transform::{JsonFlattenTool, JsonUnflattenTool};
pub use json_verify::JsonVerifyTool;
pub use locale_lookup::LocaleLookupTool;
pub use fetch_tool::FetchTool;
pub use file_adapter::{create_file_tool_adapters, FileToolAdapter};
pub use introspect_dag::DagInfoTool;
pub use introspect_orchestrate::OrchestrateTool;
pub use introspect_task::TaskStatusTool;
pub use introspect_threads::ThreadsTool;
pub use log::{LogLevel, LogTool};
pub use prompt::{PromptParams, PromptResponse, PromptTool};
pub use r#trait::BuiltinTool;
pub use records::RecordsTool;
pub use rig_adapter::NikaBuiltinToolAdapter;
pub use router::BuiltinToolRouter;
pub use run::{RunParams, RunResponse, RunTool};
pub use sleep::SleepTool;
pub use yaml_validate::YamlValidateTool;

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
