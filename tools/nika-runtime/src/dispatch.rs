// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb dispatch — the 5-arm exhaustive match (ADR-001).
//!
//! # Design (ADR-001)
//!
//! - No `trait Verb` — enum dispatch with free functions per crate
//! - The verb set is **closed**: 5 verbs, forever
//! - Each arm calls `nika_verb_*::run()` via the adapter in `verb_*.rs`
//! - `ResolvedAction` accepts pre-built typed inputs — template resolution
//!   stays in nika-engine; dispatch only executes
//!
//! # Session 21 state
//!
//! - `Exec` → **LIVE** — delegates to `verb_exec::run_exec()`
//! - `Invoke` → **LIVE** — delegates to `verb_invoke::run_invoke()`
//! - `Fetch` → `NotImplemented` (needs engine fetch bridge migration)
//! - `Infer` → `NotImplemented` (needs engine infer bridge migration)
//! - `Agent` → `NotImplemented` (no `nika-verb-agent` crate yet)

use crate::capabilities::VerbCapabilities;
use crate::error::RuntimeError;

use nika_verb_exec::ExecInput;
use nika_verb_fetch::FetchInput;
use nika_verb_infer::InferInput;
use nika_verb_invoke::InvokeInput;

/// A fully-resolved verb action ready for execution.
///
/// The engine builds this after template resolution, security validation,
/// and type coercion. `dispatch()` only executes — no parsing or resolution.
///
/// # Lifetime `'a`
///
/// Borrows resolved strings (command, url, prompt, model) from the engine's
/// resolution buffer. The buffer lives as long as the task execution.
pub enum ResolvedAction<'a> {
    /// Shell command execution.
    Exec(ExecInput<'a>),
    /// HTTP fetch with optional extraction.
    Fetch(FetchInput<'a>),
    /// MCP tool call or builtin tool invocation.
    Invoke(InvokeInput),
    /// LLM inference (simple text path).
    Infer(InferInput<'a>),
    /// Multi-turn agent loop (no `nika-verb-agent` crate yet).
    Agent,
}

/// Dispatch a resolved action to the appropriate verb crate.
///
/// Returns the verb output as a JSON value. String-producing verbs
/// (exec, fetch, invoke) return `Value::String`. Richer verbs (infer)
/// will return structured output once their arms go live.
///
/// # Live arms (S21)
///
/// - `Exec` — calls `verb_exec::run_exec`, returns stdout as `Value::String`
/// - `Invoke` — calls `verb_invoke::run_invoke`, returns tool output as `Value::String`
///
/// # Pending arms
///
/// - `Fetch` — returns `RuntimeError::NotImplemented`
/// - `Infer` — returns `RuntimeError::NotImplemented`
/// - `Agent` — returns `RuntimeError::NotImplemented`
pub async fn dispatch(
    action: &ResolvedAction<'_>,
    caps: &VerbCapabilities,
) -> Result<serde_json::Value, RuntimeError> {
    let event_log = caps.event_log();

    match action {
        ResolvedAction::Exec(input) => {
            let result = crate::verb_exec::run_exec(input, caps, event_log).await?;
            Ok(serde_json::Value::String(result))
        }
        ResolvedAction::Invoke(input) => {
            let result = crate::verb_invoke::run_invoke(input, caps, event_log).await?;
            Ok(serde_json::Value::String(result))
        }
        ResolvedAction::Fetch(_) => Err(RuntimeError::NotImplemented { verb: "fetch" }),
        ResolvedAction::Infer(_) => Err(RuntimeError::NotImplemented { verb: "infer" }),
        ResolvedAction::Agent => Err(RuntimeError::NotImplemented { verb: "agent" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn dispatch_exec_runs_command() {
        use nika_kernel_mock::shell::MockShell;
        let mock_shell = Arc::new(MockShell::new());
        mock_shell.enqueue_ok("hello");
        let caps = test_capabilities_with_shell(mock_shell);
        let input = ExecInput {
            command: "echo hello",
            shell: false,
            cwd: None,
            timeout: None,
            env: vec![],
            env_remove: vec![],
            max_stdout: None,
            task_id: Arc::from("test_exec"),
        };
        let action = ResolvedAction::Exec(input);
        let result = dispatch(&action, &caps).await;
        assert!(result.is_ok(), "exec dispatch should succeed: {result:?}");
        let val = result.unwrap();
        assert_eq!(val, serde_json::Value::String("hello".to_string()));
    }

    #[tokio::test]
    async fn dispatch_invoke_builtin_runs() {
        let caps = test_capabilities();
        let input = InvokeInput {
            tool: Some("nika:unknown_tool".to_string()),
            resource: None,
            mcp_server: None,
            params: None,
            task_id: Arc::from("test_invoke"),
            call_id: "call-1".to_string(),
        };
        let action = ResolvedAction::Invoke(input);
        let result = dispatch(&action, &caps).await;
        // NoopBuiltinRouter returns an error for any tool
        assert!(result.is_err(), "noop router should fail");
    }

    #[tokio::test]
    async fn dispatch_fetch_not_implemented() {
        let caps = test_capabilities();
        let input = FetchInput {
            url: "https://example.com",
            method: nika_kernel::http::HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout: None,
            follow_redirects: true,
            extract: None,
            extract_selector: None,
            task_id: Arc::from("test_fetch"),
        };
        let action = ResolvedAction::Fetch(input);
        let result = dispatch(&action, &caps).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("fetch"));
    }

    #[tokio::test]
    async fn dispatch_infer_not_implemented() {
        let caps = test_capabilities();
        let input = InferInput {
            prompt: "hello",
            system: None,
            model: "test-model",
            temperature: None,
            max_tokens: None,
            thinking_budget: None,
            extra: nika_kernel::provider::ProviderExtras::default(),
            task_id: Arc::from("test_infer"),
        };
        let action = ResolvedAction::Infer(input);
        let result = dispatch(&action, &caps).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("infer"));
    }

    #[tokio::test]
    async fn dispatch_agent_not_implemented() {
        let caps = test_capabilities();
        let action = ResolvedAction::Agent;
        let result = dispatch(&action, &caps).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("agent"));
    }

    /// Compile-time proof that `dispatch` returns a Send future.
    #[test]
    fn dispatch_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}

        let _assert = |action: &'static ResolvedAction<'static>,
                       caps: &'static VerbCapabilities| {
            let fut = dispatch(action, caps);
            assert_send(&fut);
        };
    }

    /// Build VerbCapabilities with a custom shell for exec tests.
    fn test_capabilities_with_shell(
        shell: Arc<dyn nika_kernel::shell::ShellExecutor>,
    ) -> VerbCapabilities {
        use nika_kernel_mock::clock::MockClock;
        use nika_kernel_mock::filesystem::InMemoryFs;
        use nika_kernel_mock::http::MockHttpClient;
        use nika_kernel_mock::mcp::MockMcpPool;
        use nika_kernel_mock::policy::MockPolicyChecker;
        use nika_kernel_mock::store::MemoryBlobStore;

        VerbCapabilities {
            shell,
            http: Arc::new(MockHttpClient::default()),
            blobs: Arc::new(MemoryBlobStore::default()),
            clock: Arc::new(MockClock::new()),
            fs_read: Arc::new(InMemoryFs::new()),
            fs_write: Arc::new(InMemoryFs::new()),
            policy: Arc::new(MockPolicyChecker::allow_all()),
            event_log: nika_event::EventLog::new(),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            builtin_router: Arc::new(NoopBuiltinRouter),
            mcp_pool: Arc::new(MockMcpPool::new()),
            provider: Arc::new(nika_kernel_mock::MockProvider::new("test")),
            workflow_base_dir: std::path::PathBuf::from("/tmp/test"),
            working_dir_mode: None,
            project_root: None,
        }
    }

    /// Build a minimal VerbCapabilities for testing dispatch routing.
    fn test_capabilities() -> VerbCapabilities {
        use nika_kernel_mock::clock::MockClock;
        use nika_kernel_mock::filesystem::InMemoryFs;
        use nika_kernel_mock::http::MockHttpClient;
        use nika_kernel_mock::mcp::MockMcpPool;
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
            mcp_pool: Arc::new(MockMcpPool::new()),
            provider: Arc::new(nika_kernel_mock::MockProvider::new("test")),
            workflow_base_dir: std::path::PathBuf::from("/tmp/test"),
            working_dir_mode: None,
            project_root: None,
        }
    }

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
}
