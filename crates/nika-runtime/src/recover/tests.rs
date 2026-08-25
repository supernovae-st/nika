// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The recover-await proofs (#291 · spec 05 §recover) — split out of the
//! crate-root `tests.rs` at the C2 wall (the 1500-LOC file ratchet).
//! `crate::*` still resolves every fixture the door shared — semantics
//! unchanged.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

// ─── on_error.recover awaits a no-edge referent (#291 · spec 05 §recover) ───
//
// A `recover: ${{ tasks.X.output }}` reference is NOT an execution-order
// edge; resolution happens at RECOVERY time; a pending/running referent is
// AWAITED to its terminal state (deterministic · never a race). The await
// rides the ordered settle spine: the failing task PARKS, every subsequent
// settlement retries covered parks, and a workflow-end pass resolves the
// rest against the final records (a still-parked referent reads as its
// pre-recovery FAILED record — recovery never rewrites the referent's
// history).

use std::num::NonZeroUsize;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use crate::*;

/// The real parse → check → run chain over mock seams (the `spec_v2`
/// harness idiom, exec-only — every fixture here drives the shell).
async fn run_yaml(yaml: &str, shell: MockShell, cap: Option<usize>) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        yaml,
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
        nika_verb_infer::InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::new(cap.and_then(NonZeroUsize::new), 0),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, sink.into_events())
}

/// Position of the FIRST frame of `kind` for `task` in the stream.
fn frame_at(events: &[Event], kind: EventKind, task: &str) -> usize {
    events
        .iter()
        .position(|e| {
            e.kind == kind
                && e.fields.iter().any(
                    |f| matches!(&f.value, FieldValue::String(s) if f.key == "task" && s == task),
                )
        })
        .unwrap_or_else(|| panic!("no {kind:?} frame for `{task}`"))
}

fn recovered_code<'e>(events: &'e [Event], task: &str) -> &'e str {
    let frame = &events[frame_at(events, EventKind::TaskRecovered, task)];
    frame
        .fields
        .iter()
        .find_map(|f| match (&f.key[..], &f.value) {
            ("code", FieldValue::String(s)) => Some(s.as_str()),
            _ => None,
        })
        .expect("task_recovered carries what it recovered FROM")
}

/// (a) Same-wave no-edge referent: the recovery AWAITS the referent's
/// terminal state and succeeds with its value — never NIKA-VAR-001,
/// never a race. The parked task's story (started → recovered →
/// completed) lands AFTER the referent settles; `output:` bindings
/// evaluate over the recovered value; downstream consumes it. The
/// stream is byte-identical for any wave-parallelism cap.
#[tokio::test]
async fn same_wave_noedge_recover_awaits_the_referent() {
    let yaml = r#"
nika: recover-await-same-wave
permits: { exec: true }
tasks:
  risky:
    exec: { shell: "exit 1" }
    on_error:
      recover: ${{ tasks.source.output }}
    extract:
      v: "."
  source:
    exec: { command: ["echo", "99"] }
  sink:
    with: { v: "${{ tasks.risky.output }}" }
    exec: { command: ["use", "${{ with.v }}"] }
"#;
    let mut streams: Vec<Vec<Event>> = Vec::new();
    for cap in [Some(1), Some(2), None] {
        let shell = MockShell::new()
            .enqueue_fail(1, "boom")
            .enqueue_ok("99\n")
            .enqueue_ok("used\n");
        let (outcome, events) = run_yaml(yaml, shell, cap).await;

        assert!(outcome.ok, "the recovery repaired the run (cap {cap:?})");
        assert_eq!(outcome.records["risky"].status, TaskStatus::Success);
        assert_eq!(outcome.records["risky"].output, Value::String("99".into()));
        assert_eq!(
            outcome.records["risky"].named["v"],
            Value::String("99".into()),
            "output: bindings evaluate over the RECOVERED value (spec 05)"
        );
        assert_eq!(outcome.records["sink"].status, TaskStatus::Success);

        // The await is visible in the stream: the referent's terminal
        // precedes the parked task's whole story.
        let source_done = frame_at(&events, EventKind::TaskCompleted, "source");
        let risky_started = frame_at(&events, EventKind::TaskStarted, "risky");
        assert!(
            source_done < risky_started,
            "the parked story lands after the awaited referent settles"
        );
        let rec = frame_at(&events, EventKind::TaskRecovered, "risky");
        let done = frame_at(&events, EventKind::TaskCompleted, "risky");
        assert!(rec < done, "task_recovered inserts before the terminal");
        assert_eq!(recovered_code(&events, "risky"), "NIKA-EXEC-001");
        streams.push(events);
    }
    assert!(
        streams.windows(2).all(|w| w[0] == w[1]),
        "the cap never leaks into the stream (ordered-settlement law)"
    );
}

/// (b) Later-wave referent + a parked-on-parked chain: A awaits B,
/// B awaits C (a later wave). C settles → B resolves → A resolves on
/// the same spine (the retry pass drains transitively). A's recovered
/// value is B's POST-recovery output — downstream of a recovered task
/// sees `status: success` + the recover value (spec 05).
#[tokio::test]
async fn later_wave_referent_resolves_transitively_on_the_spine() {
    let yaml = r#"
nika: recover-await-chain
permits: { exec: true }
tasks:
  a:
    exec: { shell: "exit 1" }
    on_error:
      recover: ${{ tasks.b.output }}
  b:
    exec: { shell: "exit 2" }
    on_error:
      recover: ${{ tasks.c.output }}
  base:
    exec: { command: ["echo", "base"] }
  c:
    after: { base: success }
    exec: { command: ["echo", "42"] }
"#;
    let shell = MockShell::new()
        .enqueue_fail(1, "a boom")
        .enqueue_fail(2, "b boom")
        .enqueue_ok("base\n")
        .enqueue_ok("42\n");
    let (outcome, events) = run_yaml(yaml, shell, Some(1)).await;

    assert!(outcome.ok);
    assert_eq!(outcome.records["b"].status, TaskStatus::Success);
    assert_eq!(outcome.records["b"].output, Value::String("42".into()));
    assert_eq!(
        outcome.records["a"].output,
        Value::String("42".into()),
        "a sees b's POST-recovery output (downstream-of-recovered law)"
    );
    let c_done = frame_at(&events, EventKind::TaskCompleted, "c");
    let b_started = frame_at(&events, EventKind::TaskStarted, "b");
    let a_started = frame_at(&events, EventKind::TaskStarted, "a");
    assert!(c_done < b_started, "b resolves after c settles");
    assert!(b_started < a_started, "the chain drains in coverage order");
}

/// (c) A referent skipped by `when:` reaches a terminal state whose
/// output is defined-null (spec 04) — the awaited recovery resolves
/// with `null`, it does NOT fail (the records hold what they hold).
#[tokio::test]
async fn skipped_referent_resolves_to_defined_null() {
    let yaml = r#"
nika: recover-await-skipped
permits: { exec: true }
tasks:
  risky:
    exec: { shell: "exit 1" }
    on_error:
      recover: ${{ tasks.source.output }}
  base:
    exec: { command: ["echo", "base"] }
  source:
    after: { base: success }
    when: false
    exec: { command: ["echo", "never"] }
"#;
    let shell = MockShell::new()
        .enqueue_fail(1, "boom")
        .enqueue_ok("base\n");
    let (outcome, events) = run_yaml(yaml, shell, Some(1)).await;

    assert!(outcome.ok, "recovered-with-null is a success");
    assert_eq!(outcome.records["risky"].status, TaskStatus::Success);
    assert_eq!(outcome.records["risky"].output, Value::Null);
    assert_eq!(recovered_code(&events, "risky"), "NIKA-EXEC-001");
    let skipped = frame_at(&events, EventKind::TaskSkipped, "source");
    let risky_started = frame_at(&events, EventKind::TaskStarted, "risky");
    assert!(skipped < risky_started, "the skip is the awaited terminal");
}

/// (d) Mutual recovery (A recovers-from B, B recovers-from A, no
/// edges): both park, both resolve at WORKFLOW-END, and each renders
/// against the other's PRE-recovery FAILED record (recovery never
/// rewrites the referent's history) — `tasks.other.status` reads
/// `failure`, not the `success` both end up with.
#[tokio::test]
async fn mutual_recovery_resolves_at_workflow_end_against_failed_records() {
    let yaml = r#"
nika: recover-await-mutual
permits: { exec: true }
tasks:
  a:
    exec: { shell: "exit 1" }
    on_error:
      recover: ${{ tasks.b.status }}
  b:
    exec: { shell: "exit 2" }
    on_error:
      recover: ${{ tasks.a.status }}
"#;
    let shell = MockShell::new()
        .enqueue_fail(1, "a boom")
        .enqueue_fail(2, "b boom");
    let (outcome, events) = run_yaml(yaml, shell, Some(1)).await;

    assert!(outcome.ok, "both recoveries resolve — nothing hangs");
    for id in ["a", "b"] {
        assert_eq!(outcome.records[id].status, TaskStatus::Success);
        assert_eq!(
            outcome.records[id].output,
            Value::String("failure".into()),
            "`{id}` sees the OTHER's pre-recovery failed state"
        );
        assert_eq!(recovered_code(&events, id), "NIKA-EXEC-001");
    }
    // Deterministic order: the end pass settles in task-id order.
    let a_done = frame_at(&events, EventKind::TaskCompleted, "a");
    let b_started = frame_at(&events, EventKind::TaskStarted, "b");
    assert!(a_done < b_started, "workflow-end resolution is id-ordered");
}

/// (e) The await gates on « not yet terminal » ONLY: a recover whose
/// reference breaks INSIDE an already-terminal referent (the path,
/// not the task, is unresolved) keeps today's fail-fast — no park is
/// owed to a referent that already settled.
#[tokio::test]
async fn broken_path_into_a_terminal_referent_still_fails_fast() {
    let yaml = r#"
nika: recover-terminal-broken-path
permits: { exec: true }
tasks:
  done:
    exec: { command: ["echo", "ok"] }
  risky:
    after: { done: success }
    exec: { shell: "exit 1" }
    on_error:
      recover: ${{ tasks.done.output.missing }}
"#;
    let shell = MockShell::new().enqueue_ok("ok\n").enqueue_fail(1, "boom");
    let (outcome, _events) = run_yaml(yaml, shell, Some(1)).await;

    assert!(
        !outcome.ok,
        "the recovery failed — the task fails as-if unhandled"
    );
    assert_eq!(outcome.records["risky"].status, TaskStatus::Failure);
    let error = outcome.records["risky"]
        .error
        .as_ref()
        .expect("error readable");
    assert!(
        error.code.starts_with("NIKA-VAR-"),
        "the render failure surfaces its spec class · got {}",
        error.code
    );
}

/// (e·unknown-name) An awaited root that is NOT a declared task can
/// never reach a terminal state — the park validation settles the
/// classification-time NIKA-VAR-001 immediately, exactly as if
/// nothing had parked. (Driven at the park site: `nika check`
/// rejects an undeclared recover ref before a run can exist, so the
/// runtime backstop is what this pins.)
#[test]
fn undeclared_awaited_root_fails_fast_at_the_park_site() {
    let yaml = "nika: t\ntasks:\n  risky:\n    exec: { shell: \"exit 1\" }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let (vars, secrets) = (BTreeMap::new(), BTreeMap::new());
    let resume_ctx = resume::ResumeContext::of(
        &wf,
        &secrets,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        None,
    );
    let scope = crate::recover::ResolveScope {
        wf: &wf,
        inputs: &vars,
        consts: &BTreeMap::new(),
        secrets: &secrets,
        resume_ctx: &resume_ctx,
        jq_clock: nika_cap::JqClock::at(nika_types::timestamp::Timestamp::EPOCH),
        run_start: nika_kernel::tool_executor::ToolRunStart::new(0),
    };
    let error =
        |code: &str, message: &str| crate::record::TaskErrorRecord::new(code, message, false);
    let pending = crate::recover::PendingRecovery {
        failed: task::FailedOutcome {
            record: error("NIKA-EXEC-001", "exit 1"),
            cost_usd: None,
            cost_unpriced: None,
            evidence: None,
        },
        render_error: error("NIKA-VAR-001", "unresolved reference `tasks.ghost.output`"),
        awaiting: std::collections::BTreeSet::from(["ghost".to_owned()]),
        with_ns: BTreeMap::new(),
    };
    let finish = task::Finish {
        id: "risky".to_owned(),
        settle: task::SettleAs::Ran(Box::new(task::RanTask {
            decisions: Vec::new(),
            note: "exec · sh".to_owned(),
            retries: Vec::new(),
            agent_events: Vec::new(),
            evidence: None,
            duration_ms: 0,
            result: task::RunResult::PendingRecovery(Box::new(pending)),
        })),
        named: BTreeMap::new(),
        resume: None,
        integrity: nika_cap::Integrity::trusted(),
        declassified: Vec::new(),
        approval: None,
    };
    let mut parked = crate::recover::ParkedRecoveries::new();
    // The streamed-wave shape: prior-wave records + the wave's side
    // map — both empty here (the downgrade needs neither).
    let prior = BTreeMap::new();
    let mut records = BTreeMap::new();
    let mut ok = true;
    let mut cache_hits = Vec::new();
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    crate::recover::settle_or_park(
        finish,
        &scope,
        &mut parked,
        &prior,
        &mut records,
        &mut ok,
        &mut cache_hits,
        &mut stamper,
        &mut sink,
    );

    assert!(!ok, "the recovery failed — nothing parked");
    assert_eq!(records["risky"].status, crate::record::TaskStatus::Failure);
    assert_eq!(
        records["risky"]
            .error
            .as_ref()
            .expect("error readable")
            .code,
        "NIKA-VAR-001"
    );
    assert!(
        sink.events()
            .iter()
            .any(|e| e.kind == EventKind::TaskFailed),
        "the terminal frame emitted at the park site"
    );
}

/// (e·fan-out) A `for_each` iteration's recover keeps today's
/// immediate resolution — iterations never park (the fan-out settles
/// as ONE task; its recovery references resolve against the wave
/// records, and a pending sibling stays NIKA-VAR-001 there).
#[tokio::test]
async fn fan_out_iteration_recover_keeps_todays_fail_fast() {
    let yaml = r#"
nika: recover-fanout-boundary
permits: { exec: true }
tasks:
  fan:
    for_each: { items: ["x"] }
    exec: { shell: "exit 1" }
    on_error:
      recover: ${{ tasks.source.output }}
  source:
    exec: { command: ["echo", "9"] }
"#;
    let shell = MockShell::new().enqueue_fail(1, "boom").enqueue_ok("9\n");
    let (outcome, _events) = run_yaml(yaml, shell, Some(1)).await;

    assert!(!outcome.ok);
    assert_eq!(outcome.records["fan"].status, TaskStatus::Failure);
    assert_eq!(
        outcome.records["fan"].error.as_ref().expect("error").code,
        "NIKA-VAR-001",
        "an iteration's unresolved recover ref stays fail-fast"
    );
    assert_eq!(outcome.records["source"].status, TaskStatus::Success);
}

/// Concatenate every string field in the journal — the cheap stand-in
/// for `grep` over the NDJSON. Item identity must appear HERE, not
/// only in an in-memory struct the trace never saw (#1077).
fn journal_text(events: &[Event]) -> String {
    let mut out = String::new();
    for event in events {
        for field in &event.fields {
            if let FieldValue::String(s) = &field.value {
                out.push_str(s);
                out.push('\n');
            }
        }
    }
    out
}

/// #1077 · three items, all fail: the parent error names an item (not
/// a count), and the fan note / the journal name the values. Deleting
/// the item from the error message fails this test.
#[tokio::test]
async fn for_each_failure_names_an_item() {
    let yaml = r#"
nika: for-each-probe
permits: { exec: true }
const:
  items: ["alpha", "beta", "gamma"]
tasks:
  each:
    for_each: { items: "${{ const.items }}", fail_fast: false }
    exec: { command: ["false"] }
"#;
    let shell = MockShell::new()
        .enqueue_fail(1, "exploded")
        .enqueue_fail(1, "exploded")
        .enqueue_fail(1, "exploded");
    let (outcome, events) = run_yaml(yaml, shell, Some(1)).await;

    assert!(!outcome.ok);
    let err = outcome.records["each"]
        .error
        .as_ref()
        .expect("a failed fan carries the first iteration error");
    assert_eq!(err.code, "NIKA-EXEC-001", "the original wire code stays");
    assert!(
        err.message.contains("alpha")
            || err.message.contains("beta")
            || err.message.contains("gamma"),
        "the Failed message names an item, not a count: {}",
        err.message
    );
    assert!(
        err.message.contains("for_each item ["),
        "index + item, so a count-only rewrite fails: {}",
        err.message
    );

    let started = events
        .iter()
        .find(|e| {
            e.kind == EventKind::TaskStarted
                && e.fields.iter().any(|f| {
                    f.key == "task" && matches!(&f.value, FieldValue::String(s) if s == "each")
                })
        })
        .expect("each started");
    let note = started
        .fields
        .iter()
        .find_map(|f| match (&f.key[..], &f.value) {
            ("note", FieldValue::String(s)) => Some(s.as_str()),
            _ => None,
        })
        .expect("TaskStarted carries the fan note");
    assert!(
        note.contains("alpha") && note.contains("beta") && note.contains("gamma"),
        "the fan note names every failed item: {note}"
    );
    assert!(
        note.contains("3 of 3 items failed"),
        "the tally is of named items: {note}"
    );

    let journal = journal_text(&events);
    assert!(
        journal.contains("alpha") && journal.contains("gamma"),
        "the trace records item identity, not only a cardinality:\n{journal}"
    );
}

/// #1077 · `fail_fast: false` with one death: the named item is the
/// failing one, not a count-only string.
#[tokio::test]
async fn for_each_fail_fast_false_names_the_failing_item() {
    let yaml = r#"
nika: fan-one-death
permits: { exec: true }
const:
  items: ["alpha", "beta", "gamma"]
tasks:
  each:
    for_each: { items: "${{ const.items }}", fail_fast: false }
    exec: { command: ["do", "${{ item }}"] }
"#;
    let shell = MockShell::new()
        .enqueue_ok("ok-alpha\n")
        .enqueue_fail(1, "exploded")
        .enqueue_ok("ok-gamma\n");
    let (outcome, events) = run_yaml(yaml, shell, Some(1)).await;

    assert!(!outcome.ok);
    let err = outcome.records["each"]
        .error
        .as_ref()
        .expect("the parent reports the failed iteration");
    assert_eq!(err.code, "NIKA-EXEC-001");
    assert!(
        err.message.contains("beta"),
        "the named item is the failing one: {}",
        err.message
    );
    assert!(
        !err.message.contains("alpha") && !err.message.contains("gamma"),
        "survivors must not be blamed: {}",
        err.message
    );

    let journal = journal_text(&events);
    assert!(
        journal.contains("beta"),
        "the trace names the failing item:\n{journal}"
    );
    assert!(
        journal.contains("1 of 3 items failed: beta"),
        "the fan note is not a count-only string:\n{journal}"
    );
}

/// #1042 · `on_error: skip` used to leave a positional null and drop
/// the item. The parent note names the recovered value, and the
/// original error (with the item) is readable at `tasks.X.error`.
#[tokio::test]
async fn for_each_skip_preserves_the_named_item() {
    let yaml = r#"
nika: fe-error-identity
permits: { exec: true }
const:
  items: ["alpha", "beta", "gamma"]
tasks:
  process:
    for_each: { items: "${{ const.items }}", fail_fast: false }
    on_error: { skip: true }
    exec: { command: ["do", "${{ item }}"] }
"#;
    let shell = MockShell::new()
        .enqueue_ok("ok-alpha\n")
        .enqueue_ok("ok-beta\n")
        .enqueue_fail(1, "exploded");
    let (outcome, events) = run_yaml(yaml, shell, Some(1)).await;

    assert!(outcome.ok, "per-iteration skip keeps the parent successful");
    assert_eq!(outcome.records["process"].status, TaskStatus::Success);
    assert_eq!(
        outcome.records["process"].output,
        serde_json::json!(["ok-alpha", "ok-beta", null])
    );

    let original = outcome.records["process"]
        .error
        .as_ref()
        .or(outcome.records["process"].recovered_from.as_ref())
        .expect("skip preserves the original error (spec 05 · tasks.X.error)");
    assert_eq!(original.code, "NIKA-EXEC-001");
    assert!(
        original.message.contains("gamma"),
        "the preserved error names the skipped item: {}",
        original.message
    );

    let journal = journal_text(&events);
    assert!(
        journal.contains("gamma"),
        "the trace records the skipped item, not only a positional null:\n{journal}"
    );
    assert!(
        journal.contains("1 recovered: gamma"),
        "the parent note names which item recovered:\n{journal}"
    );
}
