// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The facts of a run through the REAL parse → check → run chain
//! (spec 04 §`run` · spec 17 §the prologue): a run fired for a slot reads that
//! slot in a value and in `when:`, a catch-up reads the silence it covers, the
//! opening frame names both and where the instant came from, and a run told
//! nothing reads its own start.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use nika_event::{Event, EventKind};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{
    DeterministicStamper, MissedSlots, RunFacts, RunOutcome, Runtime, RuntimeConfig, VecSink,
};
use nika_types::resource::Value as FieldValue;
use nika_types::timestamp::Timestamp;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// The spec's fixture `core/variables/039` shape, with outputs that read the facts back.
const WORKFLOW: &str = r#"
nika: run-facts
permits: { exec: true }
tasks:
  report:
    when: ${{ run.missed == null }}
    exec: { command: ["echo", "${{ run.scheduled_for }}"] }
outputs:
  day: ${{ run.scheduled_for }}
  missed: ${{ run.missed }}
"#;

/// 2026-10-16T09:00:00Z, the slot a beat serves.
const SLOT: Timestamp = Timestamp::from_unix_ns(1_792_141_200_000_000_000);
/// 2026-10-13T09:00:00Z, the first slot of a silence ending at `SLOT`.
const FIRST: Timestamp = Timestamp::from_unix_ns(1_791_882_000_000_000_000);

async fn run(facts: Option<RunFacts>) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        WORKFLOW,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "the facts of the run pass the check");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("ok\n"))),
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
    let runtime = match facts {
        Some(facts) => runtime.with_run_facts(facts),
        None => runtime,
    };
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, sink.events().to_vec())
}

fn opening(events: &[Event]) -> &Event {
    events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .expect("opening frame")
}

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find_map(|f| match (&f.key, &f.value) {
        (k, FieldValue::String(s)) if k == key => Some(s.as_str()),
        _ => None,
    })
}

#[tokio::test]
async fn a_run_fired_for_a_slot_reads_it_and_its_opening_frame_names_it() {
    let (outcome, events) = run(Some(RunFacts::slot(SLOT))).await;
    assert!(outcome.ok, "the gate read run.missed == null as true");
    assert_eq!(outcome.outputs["day"], "2026-10-16T09:00:00.000Z");
    assert_eq!(outcome.outputs["missed"], serde_json::Value::Null);
    let started = opening(&events);
    assert_eq!(
        str_field(started, "scheduled_for"),
        Some("2026-10-16T09:00:00.000Z")
    );
    assert_eq!(str_field(started, "scheduled_for_source"), Some("slot"));
    assert_eq!(str_field(started, "missed"), None, "no silence, no claim");
}

#[tokio::test]
async fn a_catch_up_run_reads_the_silence_it_covers() {
    let facts = RunFacts::slot(SLOT).with_missed(MissedSlots::new(FIRST, SLOT, 4));
    let (outcome, events) = run(Some(facts)).await;
    assert_eq!(outcome.outputs["missed"]["count"], 4);
    assert_eq!(
        outcome.outputs["missed"]["first"],
        "2026-10-13T09:00:00.000Z"
    );
    assert_eq!(
        outcome.outputs["missed"]["last"],
        "2026-10-16T09:00:00.000Z"
    );
    let journaled: serde_json::Value = serde_json::from_str(
        str_field(opening(&events), "missed").expect("the silence is journaled"),
    )
    .expect("the journaled silence is JSON");
    assert_eq!(journaled, outcome.outputs["missed"]);
}

#[tokio::test]
async fn a_run_told_nothing_reads_its_own_start() {
    let (outcome, events) = run(None).await;
    let started = opening(&events);
    assert_eq!(str_field(started, "scheduled_for_source"), Some("start"));
    let at = str_field(started, "scheduled_for").expect("every run names its instant");
    assert_eq!(at, started.timestamp.to_string(), "the run clock's start");
    assert_eq!(outcome.outputs["day"], at);
}
