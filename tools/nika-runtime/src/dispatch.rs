// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb dispatch — the 5-arm exhaustive match (ADR-001).
//!
//! During Session 13, this function is built but NOT wired as the live
//! code path. The engine's `task_dispatch` continues to call
//! `TaskExecutor` verb methods via bridge delegation. Session 14
//! switches the Runner to call this directly.
//!
//! # Design (ADR-001)
//!
//! - No `trait Verb` — enum dispatch with free functions per crate
//! - The verb set is **closed**: 5 verbs, forever
//! - Each arm calls `nika_verb_*::run()` with the matching `*Caps<'_>`
//! - Infer and Agent arms are `todo!()` until Session 14 extracts them

use crate::capabilities::VerbCapabilities;
use crate::error::RuntimeError;

use nika_core::ast::analyzed::AnalyzedTaskAction;

/// Dispatch a single task action to the appropriate verb crate.
///
/// # Session 13 state
///
/// - `Exec` → `todo!()` (S13-B4 fills this arm)
/// - `Fetch` → `todo!()` (S13-D3 fills this arm)
/// - `Invoke` → `todo!()` (S13-C3 fills this arm)
/// - `Infer` → returns `RuntimeError::NotImplemented` (S14)
/// - `Agent` → returns `RuntimeError::NotImplemented` (S14)
///
/// This function is NOT called during S13. The engine's `task_dispatch`
/// remains the live path.
pub async fn dispatch(
    action: &AnalyzedTaskAction,
    _caps: &VerbCapabilities,
) -> Result<serde_json::Value, RuntimeError> {
    match action {
        AnalyzedTaskAction::Exec(_) => {
            // S13-B4: Exec arm — the actual call lives in
            // `crate::verb_exec::run_exec(input, caps, event_log)`, which
            // takes a pre-built `ExecInput`. This match arm cannot build
            // the `ExecInput` from `AnalyzedExecAction` directly because
            // template resolution (required) lives in nika-engine.
            //
            // Session 14 wires this fully when template resolution moves
            // to nika-runtime. Until then, the engine bridge calls
            // `nika_runtime::verb_exec::run_exec()` directly after doing
            // its own template resolution + security validation.
            Err(RuntimeError::NotImplemented { verb: "exec" })
        }
        AnalyzedTaskAction::Fetch(_) => {
            // S13-D3: nika_verb_fetch::run(...)
            todo!("S13-D3: wire nika-verb-fetch dispatch")
        }
        AnalyzedTaskAction::Invoke(_) => {
            // S13-C3: Invoke arm — the actual call lives in
            // `crate::verb_invoke::run_invoke(input, caps, event_log)`,
            // which takes a pre-built `InvokeInput`. This match arm
            // cannot build the `InvokeInput` from `AnalyzedInvokeAction`
            // directly because template resolution + type coercion live
            // in nika-engine.
            //
            // Session 14 wires this fully when template resolution moves
            // to nika-runtime. Until then, the engine bridge calls
            // `nika_verb_invoke::run()` directly for the builtin path.
            Err(RuntimeError::NotImplemented { verb: "invoke" })
        }
        AnalyzedTaskAction::Infer(_) => Err(RuntimeError::NotImplemented { verb: "infer" }),
        AnalyzedTaskAction::Agent(_) => Err(RuntimeError::NotImplemented { verb: "agent" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_core::ast::analyzed::{AnalyzedInferAction, AnalyzedTaskAction};

    #[tokio::test]
    async fn infer_returns_not_implemented() {
        let action = AnalyzedTaskAction::Infer(AnalyzedInferAction::default());
        let caps = test_capabilities();
        let result = dispatch(&action, &caps).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("infer"));
    }

    #[tokio::test]
    async fn agent_returns_not_implemented() {
        let action =
            AnalyzedTaskAction::Agent(Box::new(nika_core::ast::analyzed::AnalyzedAgentAction {
                ..Default::default()
            }));
        let caps = test_capabilities();
        let result = dispatch(&action, &caps).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("agent"));
    }

    /// Build a minimal VerbCapabilities for testing dispatch routing.
    /// Only the dispatch match arms that return errors are tested;
    /// the todo!() arms are not exercised.
    fn test_capabilities() -> VerbCapabilities {
        use nika_kernel_mock::clock::MockClock;
        use nika_kernel_mock::filesystem::InMemoryFs;
        use nika_kernel_mock::http::MockHttpClient;
        use nika_kernel_mock::policy::MockPolicyChecker;
        use nika_kernel_mock::shell::MockShell;
        use nika_kernel_mock::store::MemoryBlobStore;

        VerbCapabilities {
            shell: Arc::new(MockShell::new()),
            http: Arc::new(MockHttpClient::default()),
            blobs: Arc::new(MemoryBlobStore::default()),
            clock: Arc::new(MockClock::new()),
            fs_read: Arc::new(InMemoryFs::new()),
            fs_write: Arc::new(InMemoryFs::new()),
            policy: Arc::new(MockPolicyChecker::allow_all()),
            event_log: nika_event::EventLog::new(),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            builtin_router: Arc::new(NoopBuiltinRouter),
            mcp_pool: Arc::new(NoopMcpPool),
            workflow_base_dir: std::path::PathBuf::from("/tmp/test"),
            working_dir_mode: None,
            project_root: None,
        }
    }

    use std::sync::Arc;

    /// Noop builtin router for tests — no tools registered.
    #[derive(Debug)]
    struct NoopBuiltinRouter;

    impl nika_kernel::builtin::BuiltinRouter for NoopBuiltinRouter {
        fn dispatch<'a>(
            &'a self,
            tool: &'a str,
            _args: String,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<String, nika_kernel::builtin::BuiltinError>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                Err(nika_kernel::builtin::BuiltinError::Other {
                    tool: tool.to_string(),
                    reason: "noop router".to_string(),
                })
            })
        }

        fn knows(&self, _tool: &str) -> bool {
            false
        }
    }

    /// Noop MCP pool for tests — no servers configured.
    #[derive(Debug)]
    struct NoopMcpPool;

    impl nika_kernel::mcp::McpPool for NoopMcpPool {
        fn call_tool<'a>(
            &'a self,
            server: &'a str,
            _tool: &'a str,
            _args: serde_json::Value,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<serde_json::Value, nika_kernel::mcp::McpError>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                Err(nika_kernel::mcp::McpError::ServerNotFound {
                    server: server.to_string(),
                })
            })
        }

        fn read_resource<'a>(
            &'a self,
            uri: &'a str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<String, nika_kernel::mcp::McpError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                Err(nika_kernel::mcp::McpError::ResourceFailed {
                    uri: uri.to_string(),
                    reason: "noop pool".to_string(),
                })
            })
        }

        fn has_server(&self, _server: &str) -> bool {
            false
        }
    }
}
