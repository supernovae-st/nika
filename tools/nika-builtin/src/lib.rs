// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika Builtin Tools — 63 nika:* tools for the workflow engine.
//!
//! This crate contains the implementations of all builtin tools, organized
//! by category:
//!
//! - **Core (7):** sleep, log, emit, assert, prompt, run, complete
//! - **Data (13):** jq, map, filter, merge, inject, enrich, etc.
//! - **Data Sprint 2 (6):** json_verify, yaml_validate, locale_lookup, etc.
//! - **File (5):** read, write, edit, glob, grep
//! - **Introspection (6):** cost, records, dag_info, task_status, threads, orchestrate
//! - **Media (24):** import, thumbnail, chart, provenance, etc.
//!
//! # Architecture
//!
//! Tools implement `BuiltinTool` (defined in `nika-kernel`) and return
//! `Result<String, BuiltinError>`. The engine converts errors via
//! `impl From<BuiltinError> for NikaError`.
//!
//! The trait is **sealed** — implementations must also implement
//! `nika_kernel::builtin::__sealed::Sealed`.

// Re-export the trait and error from nika-kernel for convenience.
pub use nika_kernel::builtin::{BuiltinError, BuiltinTool, __sealed};

// ─── Core tools (6 of 7 — run stays in nika-engine until commit 12.7) ───
mod assert;
pub mod complete;
mod emit;
mod log;
pub mod prompt;
mod sleep;

pub use self::assert::AssertTool;
pub use self::complete::{
    is_completion_signal, parse_completion_response, CompleteParams, CompleteResponse, CompleteTool,
    COMPLETION_MARKER,
};
pub use self::emit::EmitTool;
pub use self::log::{LogLevel, LogTool};
pub use self::prompt::{PromptParams, PromptResponse, PromptTool};
pub use self::sleep::{SleepTool, MAX_SLEEP_DURATION};

// ─── Data tools (13 — 7 in data/ + jq, json_diff, io) ───
pub mod data;

pub use data::{
    ChunkTool, EnrichTool, FilterTool, GroupByTool, InjectTool, JqTool, JsonDiffTool,
    JsonMergeTool, MapTool, SetDiffTool, TokenCountTool, TreeDataTool, ZipTool,
};

// ─── Data Sprint 2 tools (6) ───
mod aggregate;
mod json_transform;
mod json_verify;
mod locale_lookup;
mod yaml_validate;

pub use self::aggregate::AggregateTool;
pub use self::json_transform::{JsonFlattenTool, JsonUnflattenTool};
pub use self::json_verify::JsonVerifyTool;
pub use self::locale_lookup::LocaleLookupTool;
pub use self::yaml_validate::YamlValidateTool;

// ─── Introspection tools (3 of 6 — EventLog-only, no RunContext deps) ───
mod cost;
mod introspect_dag;
mod introspect_threads;

pub use self::cost::CostTool;
pub use self::introspect_dag::DagInfoTool;
pub use self::introspect_threads::ThreadsTool;

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    /// Compile-time proof: Arc<dyn BuiltinTool> is Send + Sync.
    #[test]
    fn test_arc_dyn_builtin_tool_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<dyn BuiltinTool>>();
    }

    /// Verify that a concrete tool can be sealed and implemented in this crate.
    #[test]
    fn test_sealed_impl_in_downstream_crate() {
        struct DummyTool;
        impl __sealed::Sealed for DummyTool {}
        impl BuiltinTool for DummyTool {
            fn name(&self) -> &'static str {
                "dummy"
            }
            fn call<'a>(
                &'a self,
                _args: String,
            ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
                Box::pin(async { Ok(r#"{"ok":true}"#.to_string()) })
            }
        }

        let tool: Arc<dyn BuiltinTool> = Arc::new(DummyTool);
        assert_eq!(tool.name(), "dummy");
        assert_eq!(tool.description(), "");
    }

    /// Verify BuiltinError converts correctly.
    #[test]
    fn test_builtin_error_roundtrip() {
        let err = BuiltinError::invalid_params("nika:test", "bad args");
        assert!(err.to_string().contains("NIKA-212"));

        let err = BuiltinError::tool_error("nika:test", "failed");
        assert!(err.to_string().contains("NIKA-210"));
    }
}
