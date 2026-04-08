// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin Tools Module — 60 nika:* tools across 6 tiers
//!
//! **Core (7):** sleep, log, emit, assert, prompt, run, complete
//! **Data (12):** json_merge, set_diff, zip, map, filter, group_by,
//!   chunk, token_count, enrich, jq, tree_data, inject
//! **Data Sprint 2 (6):** json_verify, yaml_validate, locale_lookup, aggregate,
//!   json_flatten, json_unflatten
//! **File (5):** read, write, edit, glob, grep
//! **Introspection (6):** cost, records, dag_info, task_status, threads, orchestrate
//! **Media (24):** import, dimensions, thumbhash, dominant_color, thumbnail, convert,
//!   strip, metadata, optimize, svg_render, chart, phash, compare, pdf_extract,
//!   provenance, verify, qr_validate, quality, css_select, extract_metadata,
//!   extract_links, html_to_md, readability, pipeline
//!
//! # Architecture
//!
//! ```text
//! invoke: nika:sleep → BuiltinToolRouter → SleepTool.call()
//!                            │
//!                            ├── is_builtin("nika_*") = true
//!                            └── dispatch to appropriate tool
//! ```

mod aggregate;
mod assert;
mod complete;
mod cost;
mod data;
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
pub(crate) mod run;
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
pub use data::{
    ChunkTool, EnrichTool, FilterTool, GroupByTool, InjectTool, JqTool, JsonMergeTool, MapTool,
    SetDiffTool, TokenCountTool, TreeDataTool, ZipTool,
};
pub use emit::EmitTool;
pub use fetch_tool::FetchTool;
pub use file_adapter::{create_file_tool_adapters, FileToolAdapter};
pub use introspect_dag::DagInfoTool;
pub use introspect_orchestrate::OrchestrateTool;
pub use introspect_task::TaskStatusTool;
pub use introspect_threads::ThreadsTool;
pub use json_transform::{JsonFlattenTool, JsonUnflattenTool};
pub use json_verify::JsonVerifyTool;
pub use locale_lookup::LocaleLookupTool;
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
