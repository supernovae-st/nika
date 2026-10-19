// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! A task `timeout:` that cuts an `mcp:` call names the call it stopped:
//! the timeout's error details, the task's terminal frame and the run's
//! settlement (`stopped_calls`) carry it, `unknown` once the call was sent
//! and `not_sent` when it never was, whatever the task became: failed,
//! continued past (`on_error: skip`) or recovered.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nika_clock::SystemClock;
use nika_event::EventKind;
use nika_event::settlement::{StopOutcome, StoppedCall};
use nika_kernel::CancelCtx;
use nika_kernel::tool_executor::{ToolCall, ToolResult};
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::{InvokeVerb, McpPlane};

/// One slow `mcp:` task under a 200 ms `timeout:` (with `on_error`, when given).
fn slow_mcp(on_error: &str) -> String {
    format!(
        "nika: w\npermits:\n  tools: [\"mcp:s/slow\"]\ntasks:\n  a:\n    timeout: \"200ms\"\n{on_error}    invoke: {{ tool: \"mcp:s/slow\" }}\n"
    )
}

/// The same stoppable slow server arrangement for each timeout case.
fn slow_mcp_plane(begins: bool) -> McpPlane {
    McpPlane::stoppable(
        Arc::new(move |call: ToolCall, stop: &CancelCtx| {
            // `begins`: the server took the call; otherwise it still queued.
            if begins {
                let _ = stop.begin();
            }
            let started = Instant::now();
            while !stop.is_cancelled() && started.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(ToolResult::success(call.id.to_string(), "late"))
        }),
        Arc::new(Vec::new),
    )
}

#[tokio::test]
async fn a_timeout_names_the_mcp_call_it_stopped() {
    let skip = "    on_error: { skip: true }\n";
    let recover = "    on_error: { recover: \"fallback\" }\n";
    for (begins, outcome, on_error, status, terminal) in [
        (
            true,
            StopOutcome::Unknown,
            "",
            TaskStatus::Failure,
            EventKind::TaskFailed,
        ),
        (
            false,
            StopOutcome::NotSent,
            "",
            TaskStatus::Failure,
            EventKind::TaskFailed,
        ),
        (
            true,
            StopOutcome::Unknown,
            skip,
            TaskStatus::Skipped,
            EventKind::TaskSkipped,
        ),
        (
            true,
            StopOutcome::Unknown,
            recover,
            TaskStatus::Success,
            EventKind::TaskCompleted,
        ),
    ] {
        let wf = nika_schema::parse(
            &slow_mcp(on_error),
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "{:?}", report.findings);
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        invoke.install_mcp_plane(slow_mcp_plane(begins));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(registry, "mock/echo"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            SystemClock,
            RuntimeConfig::default(),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let run = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("the run settles");

        let record = &run.records["a"];
        assert_eq!(record.status, status, "{on_error}");
        let error = record
            .error
            .as_ref()
            .or(record.recovered_from.as_ref())
            .expect("the timeout");
        assert_eq!(error.code, "NIKA-TIMEOUT-001");
        let details = error.details.as_ref().expect("the stopped call");
        assert_eq!(details["stopped_call"]["tool"], "mcp:s/slow");
        assert_eq!(details["stopped_call"]["outcome"], outcome.as_str());
        let call_id = details["stopped_call"]["call_id"]
            .as_str()
            .expect("the call id")
            .to_owned();
        let frame = sink
            .events()
            .iter()
            .find(|e| e.kind == terminal)
            .expect("the task's terminal frame");
        let field: serde_json::Value =
            serde_json::from_str(frame.str_field("stopped_call").expect("the frame names it"))
                .expect("one JSON text");
        assert_eq!(
            field, details["stopped_call"],
            "the frame mirrors the error"
        );
        assert_eq!(
            run.settlement.stopped_calls,
            vec![StoppedCall::new("a", "mcp:s/slow", call_id, outcome)],
            "{on_error}"
        );
    }
}
