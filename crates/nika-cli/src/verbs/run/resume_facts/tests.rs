// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_event::stamp::{DeterministicStamper, Stamper};
use nika_event::{Event, EventKind};
use nika_kernel::tool_executor::{ToolCall, ToolErrorMeta, ToolResult};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{MissedSlots, RunFacts, RunOutcome, Runtime, RuntimeConfig, VecSink};
use nika_types::id::{EventId, ExecutionId, TraceId};
use nika_types::resource::{KeyValue, Value as FieldValue};
use nika_types::timestamp::Timestamp;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use super::from_trace;

const SLOT: Timestamp = Timestamp::from_unix_ns(1_792_141_200_000_000_000);
const FIRST: Timestamp = Timestamp::from_unix_ns(1_791_882_000_000_000_000);
const WORKFLOW: &str = r#"
nika: resume-run-facts
permits: { exec: ["echo"], tools: ["nika:prompt"] }
tasks:
  day:
    exec: { command: ["echo", "${{ run.scheduled_for }}", "${{ run.missed }}"] }
  ask:
    after: { day: success }
    invoke:
      tool: "nika:prompt"
      args: { mode: "confirm", message: "publish for ${{ run.scheduled_for }} with ${{ run.missed }}?" }
outputs:
  served: ${{ run.scheduled_for }}
  missed: ${{ run.missed }}
"#;

/// Different leg identities and opening stamps, while each stream stays deterministic.
struct LegStamper {
    inner: DeterministicStamper,
    leg: u128,
}

impl Stamper for LegStamper {
    fn next(&mut self) -> (EventId, Timestamp) {
        let (id, at) = self.inner.next();
        let offset = i64::try_from(self.leg).expect("fixture leg") * 60_000_000_000;
        (
            EventId::new(uuid::Uuid::from_u128((self.leg << 64) + id.uuid.as_u128())),
            Timestamp::from_unix_ns(offset + at.unix_ns),
        )
    }
}

fn text<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.str_field(key)
}

fn event<'a>(events: &'a [Event], kind: EventKind, task: Option<&str>) -> &'a Event {
    events
        .iter()
        .find(|event| {
            event.kind == kind && task.is_none_or(|task| text(event, "task") == Some(task))
        })
        .expect("fixture event")
}

async fn leg(
    source: &str,
    facts: Option<RunFacts>,
    number: u128,
    prior: Option<&[Event]>,
) -> (RunOutcome, Vec<Event>, Vec<ToolCall>) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("current native fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "current native fixture checks");
    let mut tools = MockToolExecutor::new();
    if prior.is_none() {
        let mut blocked = ToolResult::error("ask", "non-interactive and no default");
        blocked.error_meta = Some(ToolErrorMeta::new(
            Some("NIKA-BUILTIN-PROMPT-001".to_owned()),
            false,
        ));
        tools = tools.enqueue_ok(blocked);
    } else {
        // The resumed leg binds the answer as the prompt's `default:`; the
        // stdlib builtin then returns it on both planes.
        tools = tools.enqueue_ok(
            ToolResult::success("ask", "true").with_structured(serde_json::json!(true)),
        );
    }
    let tools = Arc::new(tools);
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let mut runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("same-day\n"))),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default().with_approval_operator("fixture".to_owned()),
    )
    .with_prompt_pause(true);
    if let Some(facts) = facts {
        runtime = runtime.with_run_facts(facts);
    }
    if let Some(prior) = prior {
        let fold = nika_dap::resume::fold_plan(prior);
        runtime = runtime
            .with_resume_plan(fold.plan)
            .with_paused_approval(fold.paused)
            .with_prompt_answers(BTreeMap::from([(
                "ask".to_owned(),
                serde_json::json!(true),
            )]))
            .with_resumed_from(nika_dap::resume::trace_run_id(prior));
    }
    let mut stamp = LegStamper {
        inner: DeterministicStamper::new(),
        leg: number,
    };
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamp, &mut sink)
        .await
        .expect("run settles");
    // Like the CLI's execution sink, every frame of a leg names that leg's
    // execution: the trace a later leg resumes from.
    let execution = ExecutionId::new(uuid::Uuid::from_u128(0xE0 + number));
    let frames = sink.events().iter().cloned();
    (
        outcome,
        frames
            .map(|event| event.with_execution(execution))
            .collect(),
        tools.captured_calls(),
    )
}

fn assert_kept_facts(first: &[Event], second: &[Event], restored: RunFacts) {
    let before = event(first, EventKind::WorkflowStarted, None);
    let after = event(second, EventKind::WorkflowStarted, None);
    assert_ne!(before.id, after.id, "the resumed leg gets a fresh identity");
    assert_ne!(
        before.execution, after.execution,
        "each leg is its own execution"
    );
    assert_ne!(
        before.timestamp, after.timestamp,
        "different opening stamps expose the regression",
    );
    for key in ["scheduled_for", "scheduled_for_source", "missed"] {
        assert_eq!(text(before, key), text(after, key), "same first-leg {key}");
    }
    let ancestor = TraceId::from(before.execution.expect("the first leg's execution")).to_string();
    let served = restored.scheduled_for.expect("instant").0.to_string();
    assert_eq!(text(after, "resumed_from"), Some(ancestor.as_str()));
    assert_eq!(text(after, "scheduled_for"), Some(served.as_str()));
    let done = event(first, EventKind::TaskCompleted, Some("day"));
    let hit = event(second, EventKind::TaskCacheHit, Some("day"));
    for key in ["def_hash", "input_hash", "output"] {
        assert_eq!(
            text(done, key).expect("original checkpoint field"),
            text(hit, key).expect("cache-hit checkpoint field"),
            "unchanged checkpoint {key}",
        );
    }
    let paused = event(first, EventKind::WorkflowPaused, Some("ask"));
    let answered = event(second, EventKind::ApprovalDecided, Some("ask"));
    assert_eq!(
        text(answered, "shown_hash").expect("bound hash"),
        text(paused, "approval_shown_hash").expect("shown hash"),
    );
    assert_eq!(text(answered, "decision"), Some("allow"));
    assert_eq!(text(answered, "source"), Some("resume"));
}

#[tokio::test]
async fn two_legs_keep_slot_silence_and_original_manual_start_without_reusing_identity() {
    let slot = RunFacts::slot(SLOT).with_missed(MissedSlots::new(FIRST, SLOT, 4));
    for facts in [Some(slot), None] {
        let (first, frames, _) = leg(WORKFLOW, facts, 1, None).await;
        assert!(first.paused.is_some(), "the first leg waits for its answer");
        let restored = from_trace(&frames)
            .expect("valid recorded facts")
            .expect("new prologue");
        if facts.is_some() {
            assert_eq!(restored, slot);
        } else {
            assert_eq!(
                restored.scheduled_for.expect("recorded start").1,
                nika_runtime::ScheduledForSource::Start,
            );
            assert_eq!(
                restored.scheduled_for.expect("recorded start").0,
                event(&frames, EventKind::WorkflowStarted, None).timestamp,
            );
            assert_eq!(restored.missed, None);
        }
        let (second, resumed, calls) = leg(WORKFLOW, Some(restored), 2, Some(&frames)).await;
        assert!(
            second.ok,
            "the originally shown answer binds on a later leg"
        );
        assert_eq!(calls.len(), 1, "one resumed prompt call: {calls:?}");
        let call = calls.first().expect("the resumed prompt call");
        assert_eq!(call.name, "nika:prompt");
        assert_eq!(
            call.input["default"],
            serde_json::json!(true),
            "the bound answer reaches the prompt"
        );
        assert_eq!(second.cache_hits, vec!["day".to_owned()]);
        assert_eq!(second.records["day"].output, first.records["day"].output);
        assert_eq!(
            second.outputs["served"],
            serde_json::json!(restored.scheduled_for.expect("instant").0.to_string()),
        );
        assert_eq!(
            second.outputs["missed"],
            facts.map_or(serde_json::Value::Null, |_| {
                serde_json::json!({"first": FIRST.to_string(), "last": SLOT.to_string(), "count": 4})
            }),
        );
        assert_kept_facts(&frames, &resumed, restored);
    }
}

#[tokio::test]
async fn retaining_run_facts_does_not_accept_a_changed_question() {
    let (first, frames, _) = leg(WORKFLOW, Some(RunFacts::slot(SLOT)), 1, None).await;
    assert!(first.paused.is_some());
    let facts = from_trace(&frames).expect("facts").expect("recorded");
    let (second, resumed, calls) = leg(
        &WORKFLOW.replace("publish for", "send for"),
        Some(facts),
        2,
        Some(&frames),
    )
    .await;
    assert!(!second.ok, "content changes still refuse");
    assert!(
        calls.is_empty(),
        "a refused answer never reaches the prompt"
    );
    assert_eq!(
        second.records["ask"].error.as_ref().expect("refusal").code,
        "NIKA-SEC-010",
    );
    let decision = event(&resumed, EventKind::ApprovalDecided, Some("ask"));
    assert_eq!(text(decision, "why"), Some("approval.content_mismatch"));
    assert_eq!(text(decision, "decision"), Some("deny"));
}

fn manifest(fields: &[(&str, &str)]) -> Vec<Event> {
    vec![
        Event::new(
            EventId::new(uuid::Uuid::nil()),
            Timestamp::EPOCH,
            EventKind::WorkflowStarted,
        )
        .with_fields(
            fields
                .iter()
                .map(|(key, value)| KeyValue::new(*key, FieldValue::String((*value).to_owned())))
                .collect(),
        ),
    ]
}

#[test]
fn absent_legacy_falls_through_but_bad_recorded_facts_refuse() {
    assert_eq!(from_trace(&[]), Ok(None));
    assert_eq!(from_trace(&manifest(&[])), Ok(None));
    let at = SLOT.to_string();
    for fields in [
        vec![("scheduled_for", at.as_str())],
        vec![("scheduled_for_source", "slot")],
        vec![("missed", "{}")],
        vec![
            ("scheduled_for", "not-an-instant"),
            ("scheduled_for_source", "slot"),
        ],
        vec![
            ("scheduled_for", "9999-01-01T00:00:00Z"),
            ("scheduled_for_source", "slot"),
        ],
        vec![
            ("scheduled_for", at.as_str()),
            ("scheduled_for_source", "unknown"),
        ],
        vec![
            ("scheduled_for", at.as_str()),
            ("scheduled_for_source", "slot"),
            ("scheduled_for_source", "start"),
        ],
        vec![
            ("scheduled_for", at.as_str()),
            ("scheduled_for_source", "slot"),
            (
                "missed",
                r#"{"first":"2026-10-13T09:00:00Z","last":"2026-10-16T09:00:00Z","count":4294967296}"#,
            ),
        ],
    ] {
        assert!(
            from_trace(&manifest(&fields)).is_err(),
            "malformed recorded facts refuse"
        );
    }
    let facts = from_trace(&manifest(&[
        ("scheduled_for", at.as_str()),
        ("scheduled_for_source", "at"),
    ]))
    .expect("native at source")
    .expect("recorded");
    assert_eq!(facts, RunFacts::at(SLOT));
    let mut non_string = manifest(&[("scheduled_for_source", "slot")]);
    non_string[0]
        .fields
        .push(KeyValue::new("scheduled_for", FieldValue::Int(0)));
    assert!(
        from_trace(&non_string).is_err(),
        "a malformed type never becomes a start"
    );
    let mut repeated = manifest(&[
        ("scheduled_for", at.as_str()),
        ("scheduled_for_source", "slot"),
    ]);
    repeated.extend(manifest(&[
        ("scheduled_for", "1970-01-01T00:00:00Z"),
        ("scheduled_for_source", "start"),
    ]));
    assert_eq!(
        from_trace(&repeated),
        Ok(Some(RunFacts::slot(SLOT))),
        "the first boot manifest owns the continued run's facts"
    );
}
