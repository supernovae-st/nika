#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! A `for_each` names every item's terminal (#1276 · #1397): the fan-out's
//! terminal frame carries `items` · one row per item in input order · the
//! failures 2..N with their codes, the ones a `fail_fast` stop never started
//! as such · so a machine reader and `trace peek` see the whole batch, not
//! only the first casualty.

use std::sync::Arc;

use nika_event::EventKind;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

const FAIL_FAST: &str = "nika: fan\npermits: { exec: [\"false\"] }\ntasks:\n  fan:\n    for_each: { items: [\"alpha\", \"beta\", \"gamma\"], max_parallel: 1, fail_fast: true }\n    exec: { command: [\"false\"] }\n";

const KEEP_GOING: &str = "nika: fan\npermits: { exec: [\"false\"] }\ntasks:\n  fan:\n    for_each: { items: [\"alpha\", \"beta\", \"gamma\"], fail_fast: false }\n    exec: { command: [\"false\"] }\n";

fn items_of(sink: &VecSink, kind: EventKind) -> Vec<serde_json::Value> {
    let text = sink
        .events()
        .iter()
        .find(|e| e.kind == kind)
        .and_then(|e| e.fields.iter().find(|f| f.key == "items"))
        .map(|f| match &f.value {
            FieldValue::String(s) => s.clone(),
            other => panic!("items is a JSON text, got {other:?}"),
        })
        .expect("the fan-out's terminal carries `items`");
    serde_json::from_str(&text).expect("a JSON array")
}

async fn run(source: &str, shell: MockShell) -> VecSink {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
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
    assert!(!outcome.ok, "a failed fan-out fails the run");
    sink
}

/// `fail_fast: true` · one item ran green, one failed, one never started ·
/// the table says which is which, in input order.
#[tokio::test]
async fn a_hard_failed_fan_out_names_every_item_terminal() {
    let shell = MockShell::new().enqueue_ok("done").enqueue_fail(1, "boom");
    let sink = run(FAIL_FAST, shell).await;
    let rows = items_of(&sink, EventKind::TaskFailed);
    assert_eq!(rows.len(), 3, "one row per item: {rows:?}");
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[0]["item"], "alpha");
    assert_eq!(rows[0]["status"], "ok");
    assert!(
        rows[0].get("code").is_none(),
        "a green row carries no error"
    );
    assert_eq!(rows[1]["item"], "beta");
    assert_eq!(rows[1]["status"], "failed");
    assert!(
        rows[1]["code"]
            .as_str()
            .is_some_and(|c| c.starts_with("NIKA-EXEC")),
        "the failed row carries its code: {}",
        rows[1]
    );
    assert!(
        rows[1]["message"]
            .as_str()
            .is_some_and(|m| m.contains("beta")),
        "the message names the item: {}",
        rows[1]
    );
    assert_eq!(rows[2]["item"], "gamma");
    assert_eq!(rows[2]["status"], "never_started");
}

/// `fail_fast: false` · every failure reaches the journal with its own code,
/// not only the first casualty (#1276).
#[tokio::test]
async fn every_failure_of_a_kept_going_fan_out_reaches_the_journal() {
    let shell = MockShell::new()
        .enqueue_fail(1, "first")
        .enqueue_fail(2, "second")
        .enqueue_fail(3, "third");
    let sink = run(KEEP_GOING, shell).await;
    let rows = items_of(&sink, EventKind::TaskFailed);
    assert_eq!(rows.len(), 3, "{rows:?}");
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["index"], index, "{row}");
        assert_eq!(row["status"], "failed", "{row}");
        assert!(
            row["code"].as_str().is_some(),
            "every failure carries its code: {row}"
        );
    }
    let messages: Vec<String> = rows
        .iter()
        .map(|r| r["message"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("second"))
            && messages.iter().any(|m| m.contains("third")),
        "failures 2..N are visible: {messages:?}"
    );
}
