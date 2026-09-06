// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The OBS-E `warning` lane — a tool's non-fatal diagnostic reaching the
//! task's terminal frame beside an unchanged value. Its own file because
//! `tests.rs` sits at the 1500-LOC file cap.

use super::*;

/// A tool's non-fatal diagnostic (`ToolResult::warning` · the seam a
/// `nika:glob` uses to name the directories it left out) reaches the
/// task's `task_completed` frame as the OBS-E `warning` field, beside a
/// value that is exactly what the tool returned. V9 wave 3 · p10: the
/// only carrier that makes a bare `succeeded` say what was dropped.
#[tokio::test]
async fn a_tools_warning_rides_the_task_completed_frame_beside_its_value() {
    use nika_kernel::tool_executor::ToolResult;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};

    let yaml = "nika: globwarn\npermits:\n  tools: [\"nika:glob\"]\n  fs: { read: [\"./items\"] }\ntasks:\n  discover:\n    invoke:\n      tool: \"nika:glob\"\n      args: { pattern: \"./items/*.md\" }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{:?}", report.findings);
    let said = "nika:glob returns files only · 1 directory also matched `./items/*.md` and was left out: ./items/item-07.md";
    let executor = MockToolExecutor::new().enqueue_ok(
        ToolResult::success("tc1", "[\"./items/item-06.md\"]")
            .with_structured(serde_json::json!(["./items/item-06.md"]))
            .with_warning(said),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::new(executor)));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
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
        MockClock::new(),
        RuntimeConfig::default().with_sandbox_root(std::path::PathBuf::from("/repo")),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run completes");
    assert!(outcome.ok, "a warning never fails the task");
    let record = outcome.records.get("discover").expect("discover settled");
    assert_eq!(
        record.output,
        serde_json::json!(["./items/item-06.md"]),
        "the value is exactly what the tool returned"
    );
    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("the terminal frame");
    let warning = completed
        .fields
        .iter()
        .find(|f| f.key == "warning")
        .expect("the OBS-E lane carries the tool's sentence");
    assert!(
        matches!(&warning.value, FieldValue::String(s) if s == said),
        "carried verbatim: {:?}",
        warning.value
    );
}
