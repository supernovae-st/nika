// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika `invoke:` verb — builtin + MCP tool routing.
//!
//! This crate contains the routing logic for the `invoke:` verb:
//! dispatching `nika:*` builtin tools through `BuiltinRouter` and
//! MCP tool calls / resource reads through `McpPool` — both as
//! kernel traits.
//!
//! ## What this crate does
//!
//! - Routes `nika:*` tools to `caps.builtin_router.dispatch(tool, args)`
//! - Routes `server::tool` / explicit `mcp` to `caps.mcp_pool.call_tool(...)`
//! - Emits `McpInvoke` / `McpResponse` events via `EventLog`
//! - Returns the tool result as a JSON string
//!
//! ## What the engine bridge does (NOT this crate)
//!
//! - Template resolution (`{{with.alias}}` → concrete values)
//! - Media processing of MCP blob responses (stays in engine for S13;
//!   extracted when `nika-media` is wired into the invoke Caps)
//! - Error mapping (`VerbInvokeError` → `NikaError`)

use std::sync::Arc;
use std::time::Instant;

use nika_event::{EventKind, EventLog};
use nika_kernel::caps::InvokeCaps;

mod error;
pub use error::VerbInvokeError;

/// Pre-validated, pre-resolved inputs for the invoke verb.
///
/// The engine bridge builds this after template resolution.
pub struct InvokeInput {
    /// Tool name — either `nika:*` (builtin) or `server::tool` (MCP).
    ///
    /// Exactly one of `tool` or `resource` must be set.
    pub tool: Option<String>,
    /// MCP resource URI.
    pub resource: Option<String>,
    /// Explicit MCP server name (used when `tool` is a bare name).
    pub mcp_server: Option<String>,
    /// Tool parameters (already template-resolved + type-coerced).
    pub params: Option<serde_json::Value>,
    /// Task ID for event emission.
    pub task_id: Arc<str>,
    /// Unique correlation ID for the invocation.
    pub call_id: String,
}

/// Strip `nika:` prefix from a builtin tool name.
fn strip_builtin_prefix(tool: &str) -> Option<&str> {
    tool.strip_prefix("nika:")
}

/// Execute an invoke verb task.
///
/// For S13, this crate handles **builtin routing only**. MCP routing
/// returns `VerbInvokeError::Validation` pointing the caller back to
/// the engine bridge, which still handles MCP inline with media
/// processing. Session 14 extends this to the full MCP path when
/// `nika-media` is wired into `InvokeCaps`.
pub async fn run(
    input: &InvokeInput,
    caps: &InvokeCaps<'_>,
    event_log: &EventLog,
) -> Result<String, VerbInvokeError> {
    // Validate: exactly one of tool/resource.
    if input.tool.is_none() && input.resource.is_none() {
        return Err(VerbInvokeError::Validation {
            reason: "invoke: task requires either 'tool' or 'resource' field".to_string(),
        });
    }

    let start_time = Instant::now();

    // Emit McpInvoke event (matches engine behavior).
    let mcp_server = input
        .mcp_server
        .clone()
        .unwrap_or_else(|| "builtin".to_string());
    event_log.emit(EventKind::McpInvoke {
        task_id: Arc::clone(&input.task_id),
        call_id: input.call_id.clone(),
        mcp_server,
        tool: input.tool.clone(),
        resource: input.resource.clone(),
        params: input.params.as_ref().map(|p| Arc::new(p.clone())),
    });

    // Builtin dispatch path (nika:*).
    if let Some(tool) = &input.tool {
        if let Some(bare_name) = strip_builtin_prefix(tool) {
            let args_str = input
                .params
                .as_ref()
                .map(|p| p.to_string())
                .unwrap_or_else(|| "{}".to_string());

            let dispatch_result: Result<String, nika_kernel::builtin::BuiltinError> = tokio::select! {
                biased;
                _ = caps.cancel.cancelled() => {
                    return Err(VerbInvokeError::Cancelled {
                        task_id: input.task_id.to_string(),
                    });
                }
                r = caps.builtin_router.dispatch(bare_name, args_str) => r,
            };

            let duration_ms = start_time.elapsed().as_millis() as u64;

            match dispatch_result {
                Ok(result) => {
                    // Try to parse as JSON; fall back to string wrapper.
                    let result_value: serde_json::Value = serde_json::from_str(&result)
                        .unwrap_or_else(|_| serde_json::Value::String(result.clone()));

                    event_log.emit(EventKind::McpResponse {
                        task_id: Arc::clone(&input.task_id),
                        call_id: input.call_id.clone(),
                        output_len: result.len(),
                        duration_ms,
                        cached: false,
                        is_error: false,
                        response: Some(Arc::new(result_value.clone())),
                    });

                    return Ok(result_value.to_string());
                }
                Err(e) => {
                    let err_val = serde_json::json!({"error": e.to_string()});
                    event_log.emit(EventKind::McpResponse {
                        task_id: Arc::clone(&input.task_id),
                        call_id: input.call_id.clone(),
                        output_len: 0,
                        duration_ms,
                        cached: false,
                        is_error: true,
                        response: Some(Arc::new(err_val)),
                    });
                    return Err(VerbInvokeError::Builtin(e));
                }
            }
        }
    }

    // Non-builtin path (MCP): for S13, this crate does NOT yet handle MCP
    // because media processing requires nika-media coupling that we're
    // not ready to add to the verb crate. The engine bridge continues
    // to handle MCP inline. Return a clear error so the bridge can
    // route accordingly.
    Err(VerbInvokeError::Validation {
        reason: "invoke: non-builtin MCP routing is handled by the engine bridge in S13"
            .to_string(),
    })
}

/// Returns `true` if the given tool name is a builtin (`nika:*`).
///
/// Callers use this to decide whether to route to `nika_verb_invoke::run`
/// (builtin path) or keep the engine's inline MCP handling.
pub fn is_builtin(tool: &str) -> bool {
    strip_builtin_prefix(tool).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;

    use nika_kernel::builtin::{BuiltinError, BuiltinRouter};
    use nika_kernel::mcp::McpPool;
    use nika_kernel_mock::clock::MockClock;
    use nika_kernel_mock::filesystem::InMemoryFs;
    use nika_kernel_mock::http::MockHttpClient;
    use nika_kernel_mock::mcp::MockMcpPool;
    use nika_kernel_mock::policy::MockPolicyChecker;
    use nika_kernel_mock::store::MemoryBlobStore;

    // ── Test doubles for router + pool ──

    #[derive(Debug, Clone)]
    struct StubBuiltinRouter {
        response: String,
    }

    impl BuiltinRouter for StubBuiltinRouter {
        fn dispatch<'a>(
            &'a self,
            _tool: &'a str,
            _args: String,
        ) -> Pin<
            Box<
                dyn std::future::Future<Output = Result<String, BuiltinError>>
                    + Send
                    + 'a,
            >,
        > {
            let response = self.response.clone();
            Box::pin(async move { Ok(response) })
        }

        fn knows(&self, _tool: &str) -> bool {
            true
        }
    }

    #[derive(Debug)]
    struct StubBuiltinRouterErr;

    impl BuiltinRouter for StubBuiltinRouterErr {
        fn dispatch<'a>(
            &'a self,
            tool: &'a str,
            _args: String,
        ) -> Pin<
            Box<
                dyn std::future::Future<Output = Result<String, BuiltinError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                Err(BuiltinError::Other {
                    tool: tool.to_string(),
                    reason: "stub failure".to_string(),
                })
            })
        }

        fn knows(&self, _tool: &str) -> bool {
            true
        }
    }

    // S15-A2: StubMcpPool removed — tests now use `nika_kernel_mock::MockMcpPool`.
    // The verb-invoke body does not call any `McpPool` method for the S13
    // builtin path, so the mock is only a type-slot to satisfy
    // `InvokeCaps::new`. Tests that exercise the MCP path properly go in
    // the adapter crate (S15-A3).

    // S16-swarm clippy sweep: this test helper takes 8 params because
    // `InvokeCaps<'a>` legitimately bundles 8 kernel trait refs
    // (router, pool, fs, http, blobs, policy, clock, cancel). A
    // wrapper struct would hide the param set and make the tests
    // less explicit. `#[allow(clippy::too_many_arguments)]` keeps
    // the baseline clean.
    #[allow(clippy::too_many_arguments)]
    fn make_caps<'a>(
        router: &'a dyn BuiltinRouter,
        pool: &'a dyn McpPool,
        fs: &'a InMemoryFs,
        http: &'a MockHttpClient,
        blobs: &'a MemoryBlobStore,
        policy: &'a MockPolicyChecker,
        clock: &'a MockClock,
        cancel: &'a tokio_util::sync::CancellationToken,
    ) -> InvokeCaps<'a> {
        InvokeCaps::new(
            router, pool, fs, fs, http, blobs, policy, clock, cancel,
        )
    }

    #[test]
    fn is_builtin_detects_prefix() {
        assert!(is_builtin("nika:log"));
        assert!(is_builtin("nika:write"));
        assert!(!is_builtin("log"));
        assert!(!is_builtin("server::tool"));
    }

    #[test]
    fn strip_prefix_removes_nika() {
        assert_eq!(strip_builtin_prefix("nika:log"), Some("log"));
        assert_eq!(strip_builtin_prefix("log"), None);
    }

    #[tokio::test]
    async fn builtin_dispatch_success() {
        let router = StubBuiltinRouter {
            response: r#"{"logged":true}"#.to_string(),
        };
        let pool = MockMcpPool::new();
        let fs = InMemoryFs::new();
        let http = MockHttpClient::default();
        let blobs = MemoryBlobStore::default();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        let caps = make_caps(&router, &pool, &fs, &http, &blobs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = InvokeInput {
            tool: Some("nika:log".to_string()),
            resource: None,
            mcp_server: None,
            params: Some(serde_json::json!({"level": "info", "message": "hi"})),
            task_id: Arc::from("t1"),
            call_id: "call-1".to_string(),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(result.is_ok(), "builtin dispatch failed: {result:?}");
        let output = result.unwrap();
        assert!(output.contains("logged"));

        // Verify McpInvoke + McpResponse events were emitted.
        let events = event_log.events();
        assert!(events
            .iter()
            .any(|e| matches!(e.kind, EventKind::McpInvoke { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.kind, EventKind::McpResponse { .. })));
    }

    #[tokio::test]
    async fn builtin_dispatch_error_returns_builtin_variant() {
        let router = StubBuiltinRouterErr;
        let pool = MockMcpPool::new();
        let fs = InMemoryFs::new();
        let http = MockHttpClient::default();
        let blobs = MemoryBlobStore::default();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        let caps = make_caps(&router, &pool, &fs, &http, &blobs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = InvokeInput {
            tool: Some("nika:broken".to_string()),
            resource: None,
            mcp_server: None,
            params: None,
            task_id: Arc::from("t2"),
            call_id: "call-2".to_string(),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(result, Err(VerbInvokeError::Builtin(_))));
    }

    #[tokio::test]
    async fn non_builtin_returns_validation_error_for_s13() {
        let router = StubBuiltinRouter {
            response: "{}".to_string(),
        };
        let pool = MockMcpPool::new();
        let fs = InMemoryFs::new();
        let http = MockHttpClient::default();
        let blobs = MemoryBlobStore::default();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        let caps = make_caps(&router, &pool, &fs, &http, &blobs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = InvokeInput {
            tool: Some("novanet::search".to_string()),
            resource: None,
            mcp_server: Some("novanet".to_string()),
            params: None,
            task_id: Arc::from("t3"),
            call_id: "call-3".to_string(),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(result, Err(VerbInvokeError::Validation { .. })));
    }

    #[tokio::test]
    async fn missing_tool_and_resource_errors() {
        let router = StubBuiltinRouter {
            response: "{}".to_string(),
        };
        let pool = MockMcpPool::new();
        let fs = InMemoryFs::new();
        let http = MockHttpClient::default();
        let blobs = MemoryBlobStore::default();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        let caps = make_caps(&router, &pool, &fs, &http, &blobs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = InvokeInput {
            tool: None,
            resource: None,
            mcp_server: None,
            params: None,
            task_id: Arc::from("t4"),
            call_id: "call-4".to_string(),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(result, Err(VerbInvokeError::Validation { .. })));
    }
}
