// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `NullToolExecutor` + `MockToolExecutor` — tool execution test doubles.
//!
//! Both implement the `Dyn` (Send-future) trait variants directly — the
//! `MockShell` precedent: `trait_variant::make` does NOT blanket the base
//! impl into its `Dyn` variant, and the verb/runtime seams consume the
//! `Dyn` side (`InvokeVerb<T: ToolExecuteDyn>`).

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;

use nika_kernel::tool_executor::{
    ToolBatchDyn, ToolCall, ToolExecError, ToolExecuteDyn, ToolResult,
};

/// No-op tool executor that always returns `NotAvailable`.
///
/// Used when tests don't exercise tools but need a value.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct NullToolExecutor;

impl NullToolExecutor {
    /// Create a new null tool executor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ToolExecuteDyn for NullToolExecutor {
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        Err(ToolExecError::NotAvailable {
            reason: format!("NullToolExecutor: tool '{}' not available", call.name),
        })
    }
}

impl ToolBatchDyn for NullToolExecutor {
    async fn execute_batch(&self, calls: Vec<ToolCall>) -> Vec<Result<ToolResult, ToolExecError>> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            results.push(self.execute(call).await);
        }
        results
    }
}

/// Programmable tool executor with FIFO response queue and call recording.
///
/// Enqueue results with `enqueue_ok` / `enqueue_err`.
/// Inspect recorded calls with `captured_calls`.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct MockToolExecutor {
    results: Arc<Mutex<VecDeque<Result<ToolResult, ToolExecError>>>>,
    calls: Arc<Mutex<Vec<ToolCall>>>,
}

impl MockToolExecutor {
    /// Create a new empty mock tool executor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a successful result (builder).
    #[must_use]
    pub fn enqueue_ok(self, result: ToolResult) -> Self {
        self.results.lock().push_back(Ok(result));
        self
    }

    /// Enqueue an error (builder).
    #[must_use]
    pub fn enqueue_err(self, error: ToolExecError) -> Self {
        self.results.lock().push_back(Err(error));
        self
    }

    /// Get all recorded tool calls.
    #[must_use]
    pub fn captured_calls(&self) -> Vec<ToolCall> {
        self.calls.lock().clone()
    }
}

impl ToolExecuteDyn for MockToolExecutor {
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        self.calls.lock().push(call);
        self.results.lock().pop_front().unwrap_or_else(|| {
            Err(ToolExecError::NotAvailable {
                reason: "MockToolExecutor: result queue exhausted".into(),
            })
        })
    }
}

impl ToolBatchDyn for MockToolExecutor {
    async fn execute_batch(&self, calls: Vec<ToolCall>) -> Vec<Result<ToolResult, ToolExecError>> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            results.push(self.execute(call).await);
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_returns_not_available() {
        let executor = NullToolExecutor::new();
        let call = ToolCall::new("tc1", "nika:read", serde_json::json!({}));
        let result = executor.execute(call).await;
        assert!(matches!(result, Err(ToolExecError::NotAvailable { .. })));
    }

    #[tokio::test]
    async fn null_batch_returns_all_not_available() {
        let executor = NullToolExecutor::new();
        let calls = vec![
            ToolCall::new("1", "a", serde_json::json!({})),
            ToolCall::new("2", "b", serde_json::json!({})),
        ];
        let results = executor.execute_batch(calls).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(Result::is_err));
    }

    #[tokio::test]
    async fn mock_enqueue_and_execute() {
        let executor =
            MockToolExecutor::new().enqueue_ok(ToolResult::success("tc1", "file contents"));
        let call = ToolCall::new("tc1", "nika:read", serde_json::json!({}));
        let result = executor.execute(call).await.unwrap();
        assert_eq!(result.content, "file contents");
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn mock_records_calls() {
        let executor = MockToolExecutor::new().enqueue_ok(ToolResult::success("tc1", "ok"));
        let _ = executor
            .execute(ToolCall::new(
                "tc1",
                "nika:read",
                serde_json::json!({"path": "/tmp"}),
            ))
            .await;
        let calls = executor.captured_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "nika:read");
    }

    #[tokio::test]
    async fn mock_empty_queue_returns_error() {
        let executor = MockToolExecutor::new();
        let result = executor
            .execute(ToolCall::new("tc1", "tool", serde_json::json!({})))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mock_fifo_ordering() {
        let executor = MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("1", "first"))
            .enqueue_ok(ToolResult::success("2", "second"));
        let r1 = executor
            .execute(ToolCall::new("1", "a", serde_json::json!({})))
            .await
            .unwrap();
        let r2 = executor
            .execute(ToolCall::new("2", "b", serde_json::json!({})))
            .await
            .unwrap();
        assert_eq!(r1.content, "first");
        assert_eq!(r2.content, "second");
    }

    #[tokio::test]
    async fn mock_enqueue_err_populates_queue() {
        let executor = MockToolExecutor::new().enqueue_err(ToolExecError::NotFound {
            name: "nika:ghost".into(),
        });
        let result = executor
            .execute(ToolCall::new("tc1", "nika:ghost", serde_json::json!({})))
            .await;
        assert!(matches!(result, Err(ToolExecError::NotFound { name }) if name == "nika:ghost"));
    }

    #[tokio::test]
    async fn execute_batch_returns_results_for_each_call() {
        let executor = MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("1", "alpha"))
            .enqueue_ok(ToolResult::success("2", "beta"));
        let calls = vec![
            ToolCall::new("1", "a", serde_json::json!({})),
            ToolCall::new("2", "b", serde_json::json!({})),
        ];
        let results = executor.execute_batch(calls).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_ref().unwrap().content, "alpha");
        assert_eq!(results[1].as_ref().unwrap().content, "beta");
    }

    #[test]
    fn null_satisfies_the_dyn_runtime_seam() {
        fn _accepts<T: ToolExecuteDyn + ToolBatchDyn>(_: &T) {}
        _accepts(&NullToolExecutor::new());
    }

    #[test]
    fn mock_satisfies_the_dyn_runtime_seam() {
        fn _accepts<T: ToolExecuteDyn + ToolBatchDyn>(_: &T) {}
        _accepts(&MockToolExecutor::new());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn tool_executor_types_send_sync() {
        _assert_send_sync::<NullToolExecutor>();
        _assert_send_sync::<MockToolExecutor>();
    }
}
