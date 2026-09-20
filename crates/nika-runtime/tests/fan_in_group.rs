#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! The fan-in fold (spec 03 §group): `${{ group.<name> }}` in a `with:`
//! value materializes at run time as ONE array of member records in
//! DECLARATION order. Before this test the checker admitted the fold (its
//! `fan-in` edges scheduled the consumer after every member) while the
//! runtime had no binding for the `group` root, so every fold died at the
//! boundary with NIKA-VAR-001 — `nika check` green, `nika run` red
//! (canary c04-fanout-fanin · 2026-09-20).

use std::sync::Arc;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// `leg_b` is DECLARED before `leg_a`: the fold must follow the source
/// order of `tasks:`, not the alphabetical order a records map would give,
/// and never completion order.
const FAN_IN: &str = "nika: fan-in\npermits: { exec: [\"echo\"] }\ntasks:\n  leg_b:\n    group: probes\n    exec: { command: [\"echo\", \"b\"] }\n  leg_a:\n    group: probes\n    exec: { command: [\"echo\", \"a\"] }\n  summary:\n    with: { legs: \"${{ group.probes }}\" }\n    exec: { command: [\"echo\", \"${{ with.legs }}\"] }\n";

#[tokio::test]
async fn a_group_fold_binds_every_member_record_in_declaration_order() {
    let wf = nika_schema::parse(
        FAN_IN,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "the fold passes the ladder: {report:?}");
    // Both legs run in the same wave and the queue answers whichever asks
    // first, so every leg gets the same stdout: the fold is judged on shape.
    let shell = Arc::new(
        MockShell::new()
            .enqueue_ok("leg")
            .enqueue_ok("leg")
            .enqueue_ok("folded"),
    );
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::clone(&shell)),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles");

    let summary = outcome.records.get("summary").expect("summary settled");
    assert_eq!(
        summary.status,
        TaskStatus::Success,
        "the fold consumer runs: {summary:?}"
    );
    assert!(outcome.ok, "a folded run settles green");

    // The rendered fold reached the verb as canonical JSON in argv.
    let commands = shell.executed_commands();
    let folded = commands
        .iter()
        .find(|c| c.args.first().is_some_and(|a| a.starts_with('[')))
        .expect("the summary argv carries the fold");
    let fold: serde_json::Value = serde_json::from_str(&folded.args[0]).expect("canonical JSON");
    let members = fold.as_array().expect("one array of member records");
    let ids: Vec<&str> = members
        .iter()
        .map(|m| m["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["leg_b", "leg_a"], "declaration order, not map order");
    for member in members {
        assert_eq!(member["status"], "success");
        assert_eq!(member["output"].as_str().map(str::trim), Some("leg"));
        assert!(member["error"].is_null(), "defined-null on a success");
        assert!(
            member["duration_ms"].is_number(),
            "a member that ran has a duration"
        );
        // The member record is CLOSED at v1 (spec 03 §the member record).
        let mut keys: Vec<&str> = member
            .as_object()
            .expect("a record")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["duration_ms", "error", "id", "output", "status"]);
    }
}
