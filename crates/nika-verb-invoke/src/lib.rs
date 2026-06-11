// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # nika-verb-invoke — the `invoke` verb executor (L2)
//!
//! Builtin (`nika:`) and MCP (`mcp:`) tool calls per
//! `nika-spec spec/02-verbs.md §invoke` — the third of the 4 verbs
//! (`infer · exec · invoke · agent` · locked forever per D-2026-05-22-N18).
//!
//! ## Shape
//!
//! - **Dispatcher injected, never owned** — the verb rides the kernel
//!   `ToolExecuteDyn` seam: production wiring injects the engine's
//!   builtin+MCP dispatcher (resolves `nika:*` against the closed 22-builtin
//!   set + `mcp:*` against the configured server registry); tests inject a
//!   mock executor. NO Cargo dep on `nika-builtin` / `nika-mcp`.
//! - **The closed-namespace contract (spec §invoke)** — the tool-ref
//!   namespace set is CLOSED at v1: `nika:` and `mcp:` only. `mcp:` REQUIRES
//!   the slash (`mcp:postgres` alone is unresolvable). The verb does this
//!   lightweight semantic check BEFORE dispatch (the grammar SHAPE is the
//!   upstream `nika-schema` `NIKA-PARSE` concern).
//!
//! ## Fences (what this crate is NOT)
//!
//! The tool implementations (behind the dispatcher) · `${{ }}` resolution
//! (upstream) · args-schema validation (the tool owns its schema · spec
//! NIKA-INVOKE-002 reserved) · batch (`ToolBatchDyn` is the agent surface).
//!
//! ## Example (mock · zero tool)
//!
//! ```
//! use std::sync::Arc;
//! use nika_verb_invoke::{InvokeInput, InvokeVerb};
//! # use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
//! # struct Echo;
//! # impl ToolExecuteDyn for Echo {
//! #     async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
//! #         Ok(ToolResult::success(call.id.to_string(), "ok"))
//! #     }
//! # }
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let verb = InvokeVerb::new(Arc::new(Echo));
//! let out = verb.run(InvokeInput::new("nika:read")).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod errors;

use std::sync::Arc;

use nika_kernel::tool_executor::{ToolCall, ToolExecuteDyn};

pub use errors::VerbInvokeError;

/// The two closed tool namespaces (spec §invoke · CLOSED at v1).
const NAMESPACES: [&str; 2] = ["nika", "mcp"];

/// The `invoke` task input — spec fields, already CEL-resolved.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InvokeInput {
    /// The tool id: `nika:<path>` or `mcp:<server>/<tool>` (required).
    pub tool: String,
    /// Tool arguments (default `{}` · tool-specific schema).
    pub args: serde_json::Value,
    /// Engine-supplied `ToolCall` id (derived from the tool id if absent).
    pub call_id: Option<String>,
}

impl InvokeInput {
    /// A no-args invocation of `tool`.
    #[must_use]
    pub fn new(tool: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            args: serde_json::Value::Object(serde_json::Map::new()),
            call_id: None,
        }
    }
}

/// The `invoke` verb output.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InvokeOutput {
    /// The tool's response content (`.output` in spec terms).
    pub content: String,
    /// The resolved tool id (echo · engine event context).
    pub tool: String,
}

impl InvokeOutput {
    /// Construct from the tool id and its response content.
    #[must_use]
    pub fn new(tool: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tool: tool.into(),
        }
    }
}

/// Builtin/MCP tool call — the `invoke` verb executor.
#[derive(Debug)]
pub struct InvokeVerb<T> {
    executor: Arc<T>,
}

impl<T> InvokeVerb<T> {
    /// Create the verb over an injected tool executor.
    #[must_use]
    pub fn new(executor: Arc<T>) -> Self {
        Self { executor }
    }
}

impl<T> InvokeVerb<T>
where
    T: ToolExecuteDyn + Sync,
{
    /// Execute the `invoke` task.
    ///
    /// CANCEL SAFETY: delegates to the tool's own contract
    /// (`ToolExecute::execute`) — per-tool, documented at the dispatcher.
    ///
    /// # Errors
    ///
    /// [`VerbInvokeError::UnresolvableTool`] when the tool id fails the
    /// closed-namespace/slash check or the dispatcher cannot find it ·
    /// [`VerbInvokeError::ToolReportedError`] when the tool returns
    /// `is_error: true` · [`VerbInvokeError::Dispatch`] on a dispatcher
    /// timeout/execution/availability failure.
    pub async fn run(&self, input: InvokeInput) -> Result<InvokeOutput, VerbInvokeError> {
        validate_tool_ref(&input.tool)?;

        let call_id = input.call_id.unwrap_or_else(|| derive_call_id(&input.tool));
        let call = ToolCall::new(call_id, input.tool.clone(), input.args);

        let result = self
            .executor
            .execute(call)
            .await
            .map_err(map_dispatch_error)?;

        if result.is_error {
            return Err(VerbInvokeError::tool_reported(input.tool, &result.content));
        }
        Ok(InvokeOutput::new(input.tool, result.content))
    }
}

/// The closed-namespace + `mcp:`-needs-slash semantic check (NIKA-450).
///
/// Grammar SHAPE (single colon, path charset) is the upstream
/// `nika-schema` `NIKA-PARSE` concern; here we reject only what makes a
/// tool id semantically unresolvable at the verb boundary.
fn validate_tool_ref(tool: &str) -> Result<(), VerbInvokeError> {
    let unresolvable = |detail: &str| VerbInvokeError::UnresolvableTool {
        tool: tool.to_owned(),
        detail: detail.to_owned(),
    };
    let Some((ns, path)) = tool.split_once(':') else {
        return Err(unresolvable("missing the `namespace:` prefix"));
    };
    if !NAMESPACES.contains(&ns) {
        return Err(unresolvable(
            "unknown namespace (the set is closed: `nika:` or `mcp:`)",
        ));
    }
    if path.is_empty() {
        return Err(unresolvable("empty tool path after the namespace"));
    }
    // `mcp:` requires `<server>/<tool>` — both segments non-empty.
    if ns == "mcp" {
        match path.split_once('/') {
            Some((server, name)) if !server.is_empty() && !name.is_empty() => {}
            _ => return Err(unresolvable("`mcp:` requires `<server>/<tool>`")),
        }
    }
    Ok(())
}

/// A stable `ToolCall` id when the engine did not supply one.
fn derive_call_id(tool: &str) -> String {
    format!("invoke:{tool}")
}

/// Map the kernel dispatcher error onto the verb surface. `NotFound` is a
/// resolution failure (NIKA-450); the rest are dispatch failures (452).
fn map_dispatch_error(source: nika_kernel::tool_executor::ToolExecError) -> VerbInvokeError {
    use nika_kernel::tool_executor::ToolExecError;
    match source {
        ToolExecError::NotFound { name } => VerbInvokeError::UnresolvableTool {
            tool: name,
            detail: "the dispatcher resolved no such tool".to_owned(),
        },
        other => VerbInvokeError::Dispatch { source: other },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
    use std::sync::Mutex;

    /// A scripted tool executor recording the calls it receives. The
    /// scripted result is single-use (`ToolExecError` is not `Clone`); the
    /// run-once tests never re-enter.
    struct MockTool {
        result: Mutex<Option<Result<ToolResult, ToolExecError>>>,
        seen: Mutex<Vec<ToolCall>>,
    }

    impl MockTool {
        fn with(result: Result<ToolResult, ToolExecError>) -> Self {
            Self {
                result: Mutex::new(Some(result)),
                seen: Mutex::new(Vec::new()),
            }
        }
        fn ok(content: &str) -> Self {
            Self::with(Ok(ToolResult::success("tc", content)))
        }
        fn err_result(content: &str) -> Self {
            Self::with(Ok(ToolResult::error("tc", content)))
        }
        fn dispatch_err(e: ToolExecError) -> Self {
            Self::with(Err(e))
        }
    }

    impl ToolExecuteDyn for MockTool {
        async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
            self.seen.lock().unwrap().push(call);
            self.result.lock().unwrap().take().unwrap_or_else(|| {
                Err(ToolExecError::NotAvailable {
                    reason: "exhausted".into(),
                })
            })
        }
    }

    fn verb(mock: MockTool) -> InvokeVerb<MockTool> {
        InvokeVerb::new(Arc::new(mock))
    }

    #[tokio::test]
    async fn nika_builtin_happy_path() {
        let out = verb(MockTool::ok("file contents"))
            .run(InvokeInput::new("nika:read"))
            .await
            .expect("builtin resolves");
        assert_eq!(out, InvokeOutput::new("nika:read", "file contents"));
    }

    #[tokio::test]
    async fn mcp_tool_happy_path_and_args_passthrough() {
        let mock = Arc::new(MockTool::ok("rows"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        let mut input = InvokeInput::new("mcp:postgres/query");
        input.args = serde_json::json!({ "sql": "SELECT 1" });
        let out = verb.run(input).await.expect("mcp resolves");
        assert_eq!(out.content, "rows");
        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen[0].name, "mcp:postgres/query");
        assert_eq!(seen[0].input, serde_json::json!({ "sql": "SELECT 1" }));
    }

    #[tokio::test]
    async fn supplied_call_id_wins_over_derived() {
        let mock = Arc::new(MockTool::ok("x"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        let mut input = InvokeInput::new("nika:read");
        input.call_id = Some("engine-tc-42".to_owned());
        verb.run(input).await.expect("ok");
        assert_eq!(mock.seen.lock().unwrap()[0].id.to_string(), "engine-tc-42");
    }

    #[tokio::test]
    async fn derived_call_id_when_absent() {
        let mock = Arc::new(MockTool::ok("x"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        verb.run(InvokeInput::new("nika:read")).await.expect("ok");
        assert_eq!(
            mock.seen.lock().unwrap()[0].id.to_string(),
            "invoke:nika:read"
        );
    }

    #[tokio::test]
    async fn unknown_namespace_is_rejected_before_dispatch() {
        let mock = Arc::new(MockTool::ok("x"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        let err = verb
            .run(InvokeInput::new("custom:thing"))
            .await
            .expect_err("closed namespace");
        assert!(matches!(err, VerbInvokeError::UnresolvableTool { .. }));
        assert!(mock.seen.lock().unwrap().is_empty(), "zero dispatch");
    }

    #[tokio::test]
    async fn mcp_without_slash_is_rejected_before_dispatch() {
        let mock = Arc::new(MockTool::ok("x"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        let err = verb
            .run(InvokeInput::new("mcp:postgres"))
            .await
            .expect_err("mcp needs slash");
        assert!(matches!(err, VerbInvokeError::UnresolvableTool { .. }));
        assert!(mock.seen.lock().unwrap().is_empty(), "zero dispatch");
    }

    #[tokio::test]
    async fn tool_reported_error_maps_to_451() {
        let err = verb(MockTool::err_result("permission denied"))
            .run(InvokeInput::new("nika:read"))
            .await
            .expect_err("is_error propagates");
        assert!(matches!(err, VerbInvokeError::ToolReportedError { .. }));
    }

    #[tokio::test]
    async fn dispatcher_notfound_maps_to_unresolvable() {
        let err = verb(MockTool::dispatch_err(ToolExecError::NotFound {
            name: "nika:ghost".to_owned(),
        }))
        .run(InvokeInput::new("nika:ghost"))
        .await
        .expect_err("not found");
        assert!(matches!(err, VerbInvokeError::UnresolvableTool { .. }));
    }

    #[tokio::test]
    async fn dispatcher_timeout_maps_to_dispatch() {
        let err = verb(MockTool::dispatch_err(ToolExecError::Timeout {
            name: "mcp:slow/op".to_owned(),
            duration_ms: 30_000,
        }))
        .run(InvokeInput::new("mcp:slow/op"))
        .await
        .expect_err("timeout");
        assert!(matches!(err, VerbInvokeError::Dispatch { .. }));
    }

    #[test]
    fn tool_ref_validation_matrix() {
        assert!(validate_tool_ref("nika:read").is_ok());
        assert!(validate_tool_ref("nika:connectome/recall").is_ok());
        assert!(validate_tool_ref("mcp:db/query").is_ok());
        for bad in [
            "", "noprefix", "custom:x", "nika:", "mcp:", "mcp:db", "mcp:/q", "mcp:db/",
        ] {
            assert!(validate_tool_ref(bad).is_err(), "{bad:?} must be rejected");
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The tool-ref validator is total: never panics on arbitrary input,
        /// and a valid `nika:`/`mcp:` shape is exactly the accept set.
        #[test]
        fn validate_tool_ref_is_total(s in ".{0,40}") {
            let ok = validate_tool_ref(&s).is_ok();
            // Cross-check against an independent predicate.
            let expected = match s.split_once(':') {
                Some(("nika", path)) => !path.is_empty(),
                Some(("mcp", path)) => path
                    .split_once('/')
                    .is_some_and(|(srv, tool)| !srv.is_empty() && !tool.is_empty()),
                _ => false,
            };
            prop_assert_eq!(ok, expected);
        }
    }
}
