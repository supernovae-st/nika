// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin Tools Module — 60 nika:* tools across 6 tiers
//!
//! **Core (7):** sleep, log, emit, assert, prompt, run, complete
//! **Data (13):** json_merge, json_diff, set_diff, zip, map, filter, group_by,
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

mod fetch_tool;
mod file_adapter;
mod introspect_orchestrate;
mod introspect_task;
pub(crate) mod media;
mod records;
mod rig_adapter;
mod router;
pub(crate) mod run;
pub(crate) mod r#trait;

// ── Re-exports from nika-builtin (6 core + 13 data + 6 sprint2 tools) ──
pub use nika_builtin::AssertTool;
pub use nika_builtin::{
    is_completion_signal, parse_completion_response, CompleteParams, CompleteResponse, CompleteTool,
    COMPLETION_MARKER,
};
pub use nika_builtin::EmitTool;
pub use nika_builtin::{LogLevel, LogTool};
pub use nika_builtin::{PromptParams, PromptResponse, PromptTool};
pub use nika_builtin::SleepTool;
pub use nika_builtin::{
    AggregateTool, ChunkTool, CostTool, DagInfoTool, EnrichTool, FilterTool, GroupByTool,
    InjectTool, JqTool, JsonDiffTool, JsonFlattenTool, JsonMergeTool, JsonUnflattenTool,
    JsonVerifyTool, LocaleLookupTool, MapTool, SetDiffTool, ThreadsTool, TokenCountTool,
    TreeDataTool, YamlValidateTool, ZipTool,
};

// ── Local exports (tools that remain in nika-engine) ──
pub use fetch_tool::FetchTool;
pub use file_adapter::{create_file_tool_adapters, FileToolAdapter};
pub use introspect_orchestrate::OrchestrateTool;
pub use introspect_task::TaskStatusTool;
pub use r#trait::BuiltinTool;
pub use records::RecordsTool;
pub use rig_adapter::NikaBuiltinToolAdapter;
pub use router::BuiltinToolRouter;
pub use run::{RunParams, RunResponse, RunTool};

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
