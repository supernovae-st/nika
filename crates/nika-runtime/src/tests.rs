// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The crate-root test module — moved out of `lib.rs` 2026-07-10 at
//! 1461/1500 LOC (the #291 recover-await design lands in that file's
//! settle spine; the headroom must exist BEFORE the feature). `super`
//! still resolves to the crate root — semantics unchanged.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::*;

/// #412 test seam — the gate: a `nika:jq` call whose `expression` arg is
/// `".gate"` polls the sink's flag (1ms yields — tokio's `sync` feature
/// is off in this workspace, and a flag poll needs only `time`); every
/// other call answers instantly. Bounded at 5s so a regression fails
/// loudly, never hangs the suite.
struct GateExecutor {
    unblock: Arc<std::sync::atomic::AtomicBool>,
}

impl nika_kernel::tool_executor::ToolExecuteDyn for GateExecutor {
    async fn execute(
        &self,
        call: nika_kernel::tool_executor::ToolCall,
    ) -> Result<nika_kernel::tool_executor::ToolResult, nika_kernel::tool_executor::ToolExecError>
    {
        use std::sync::atomic::Ordering;
        if call.input.get("expression").and_then(Value::as_str) == Some(".gate") {
            let mut waited_ms = 0u32;
            while !self.unblock.load(Ordering::Acquire) {
                if waited_ms > 5_000 {
                    return Err(nika_kernel::tool_executor::ToolExecError::NotAvailable {
                        reason: "settles did not stream — the gate starved (#412 \
                                 regression: frames held to the wave join)"
                            .into(),
                    });
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                waited_ms += 1;
            }
        }
        Ok(nika_kernel::tool_executor::ToolResult::success(
            call.id.as_str(),
            "\"done\"",
        ))
    }
}

/// #412 test seam — the observer: forwards every event to a [`VecSink`]
/// and flips the gate's flag when `fast`'s terminal frame arrives.
struct NotifyOnFastSettle {
    inner: VecSink,
    unblock: Arc<std::sync::atomic::AtomicBool>,
}

impl EventSink for NotifyOnFastSettle {
    fn emit(&mut self, event: Event) {
        let is_fast_completed = event.kind == EventKind::TaskCompleted
            && event.fields.iter().any(|kv| {
                kv.key == "task" && matches!(&kv.value, FieldValue::String(s) if s == "fast")
            });
        if is_fast_completed {
            self.unblock
                .store(true, std::sync::atomic::Ordering::Release);
        }
        self.inner.emit(event);
    }
}

/// #412 · settles STREAM through the ordered spine: a settled sibling's
/// frames reach the sink at ITS settle, not the wave join. Proof by
/// construction: `gate` (same wave, declared after `fast`) BLOCKS until
/// the sink has seen fast's `task_completed` — join-granularity frames
/// would starve it forever (they'd only exist after gate itself
/// finished); the streamed spine settles fast first and unblocks the
/// wave. A 5s timeout turns a regression into a loud failure, never a
/// hung suite.
#[tokio::test]
async fn wave_settles_stream_before_the_join() {
    use nika_kernel_mock::{MockClock, MockProvider, MockShell, MockToolDefinitionProvider};
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use std::sync::atomic::AtomicBool;

    let yaml = "nika: v1\nworkflow:\n  id: stream-settle\ntasks:\n  fast:\n    exec: { command: [\"true\"] }\n  gate:\n    invoke: { tool: \"nika:jq\", args: { input: [], expression: \".gate\" } }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_schema::check(&wf);
    assert!(report.is_clean(), "fixture must check clean");
    assert_eq!(report.waves.len(), 1, "ONE wave — the whole point");

    let unblock = Arc::new(AtomicBool::new(false));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(GateExecutor {
        unblock: Arc::clone(&unblock),
    })));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("ok"))),
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
    let mut sink = NotifyOnFastSettle {
        inner: VecSink::new(),
        unblock,
    };
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    assert!(outcome.ok, "both tasks settle green: {:?}", outcome.records);

    // The settle ORDER is unchanged (submission order — the spine):
    // fast's terminal frame precedes gate's.
    let completed: Vec<&str> = sink
        .inner
        .events()
        .iter()
        .filter(|e| e.kind == EventKind::TaskCompleted)
        .filter_map(|e| {
            e.fields
                .iter()
                .find(|kv| kv.key == "task")
                .and_then(|kv| match &kv.value {
                    FieldValue::String(s) => Some(s.as_str()),
                    _ => None,
                })
        })
        .collect();
    assert_eq!(completed, ["fast", "gate"], "the ordered spine holds");
}

#[test]
fn runtime_config_default_is_wave_width_seed_zero() {
    let cfg = RuntimeConfig::default();
    assert!(cfg.wave_parallelism.is_none());
    assert_eq!(cfg.jitter_seed, 0);
}

#[test]
fn envelope_values_carries_typed_defaults_and_containers() {
    // The v1 string-only view dropped typed list defaults — the
    // value model must carry them (for_each collections · spec 03).
    let yaml = r#"
nika: v1
workflow:
  id: vals
env:
  API_BASE: "https://api.example.test"
vars:
  plain: "text"
  urls: ["a", "b"]
  topic: { type: string, default: "news" }
tasks:
  t:
    exec: { command: ["true"] }
"#;
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    let (vars, env, name) = envelope_values(&wf, &BTreeMap::new());
    assert_eq!(name, "vals");
    assert_eq!(
        env["API_BASE"],
        Value::String("https://api.example.test".into())
    );
    assert_eq!(vars["plain"], Value::String("text".into()));
    assert_eq!(vars["urls"], serde_json::json!(["a", "b"]));
    assert_eq!(vars["topic"], Value::String("news".into()));
}

#[test]
fn typed_output_type_mismatch_is_a_var009() {
    // `outputs.n: { type: string }` — when the resolved value is a number
    // the callable contract is broken (spec 01 §engine-MUST rule 6).
    let yaml = r#"
nika: v1
workflow:
  id: typed-out
tasks:
  t:
    invoke: { tool: "nika:jq", args: { input: { x: 42 }, expression: ".x" } }
outputs:
  n:
    value: ${{ tasks.t.output }}
    type: string
"#;
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    // A number where `string` is declared → NIKA-VAR-009.
    let bad = BTreeMap::from([("n".to_owned(), serde_json::json!(42))]);
    let v =
        first_output_type_violation(&wf, &bad).expect("number vs declared string is a violation");
    assert_eq!(v.name, "n");
    assert_eq!(v.expected, "string");
    assert_eq!(v.actual, "number");
    // The declared type → no violation.
    let good = BTreeMap::from([("n".to_owned(), serde_json::json!("hello"))]);
    assert!(first_output_type_violation(&wf, &good).is_none());
    // An unresolved output (omitted upstream) is NOT a type error.
    assert!(first_output_type_violation(&wf, &BTreeMap::new()).is_none());
}

#[test]
fn value_matches_vartype_lenient_floats_strict_cross_type() {
    use serde_json::json;
    // integer: whole floats OK, fractional rejected, numeric STRING rejected.
    assert!(value_matches_vartype(&json!(42), VarType::Integer));
    assert!(value_matches_vartype(&json!(42.0), VarType::Integer));
    assert!(!value_matches_vartype(&json!(42.5), VarType::Integer));
    // number: any JSON number, but NOT a numeric string.
    assert!(value_matches_vartype(&json!(42), VarType::Number));
    assert!(!value_matches_vartype(&json!("42"), VarType::Number));
    // array vs object are distinct.
    assert!(value_matches_vartype(&json!([1, 2]), VarType::Array));
    assert!(!value_matches_vartype(&json!({}), VarType::Array));
    assert!(value_matches_vartype(&json!({ "k": 1 }), VarType::Object));
    assert!(value_matches_vartype(&json!("x"), VarType::String));
    assert!(value_matches_vartype(&json!(true), VarType::Boolean));
}

/// A recovered success emits `task_recovered` BEFORE the terminal
/// `task_completed` (engine#301 · D-2026-07-08-N4 sequence lock:
/// `… > task_recovered > task_completed`) — and carries WHAT it
/// recovered from as a `code` field. A clean success emits no such
/// frame (pinned by every other Success test in this file).
#[test]
fn recovered_success_emits_task_recovered_before_completed() {
    let ran = task::RanTask {
        note: "exec · sh".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::Number(99.into()),
            tokens: None,
            recovered_from: Some(crate::record::TaskErrorRecord {
                code: "NIKA-EXEC-001".to_owned(),
                message: "exit 9".to_owned(),
                transient: false,
            }),
            warning: None,
            cost_usd: None,
            cost_unpriced: None,
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle_ran("risky", ran, None, &mut ok, &mut stamper, &mut sink);

    let kinds: Vec<EventKind> = sink.events().iter().map(|e| e.kind).collect();
    let rec = kinds
        .iter()
        .position(|k| *k == EventKind::TaskRecovered)
        .expect("a TaskRecovered frame");
    let done = kinds
        .iter()
        .position(|k| *k == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame — completed STAYS the one success terminal");
    assert!(rec < done, "task_recovered inserts BEFORE the terminal");

    let frame = &sink.events()[rec];
    assert!(
        frame.fields.iter().any(|f| f.key == "code"
            && matches!(&f.value, FieldValue::String(s) if s == "NIKA-EXEC-001")),
        "the frame names what was recovered FROM"
    );
    assert!(ok, "a recovered task is a SUCCESS at workflow level");
}

/// A settled success carrying an OBS-E `warning` puts it on the
/// `TaskCompleted` frame as a `warning` field — the wiring proof that
/// the dispatch's diagnostic actually reaches the event stream.
#[test]
fn obs_e_warning_rides_task_completed() {
    let ran = task::RanTask {
        note: "infer · gemini/flash".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::String(String::new()),
            tokens: Some(84),
            recovered_from: None,
            warning: Some("infer produced an empty answer · …".to_owned()),
            cost_usd: Some(0.0125),
            cost_unpriced: None,
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle_ran("think", ran, None, &mut ok, &mut stamper, &mut sink);

    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    let warning = completed
        .fields
        .iter()
        .find(|f| f.key == "warning")
        .expect("the warning field rides the success frame");
    assert!(
        matches!(&warning.value, FieldValue::String(s) if s.contains("empty answer")),
        "the diagnostic text is carried verbatim"
    );
    // Real spend rides the same frame · absent-when-unpriced is pinned
    // by the sibling test below (its cost_usd is None · no field).
    let cost = completed
        .fields
        .iter()
        .find(|f| f.key == "cost_usd")
        .expect("the cost_usd field rides the priced success frame");
    assert!(
        matches!(&cost.value, FieldValue::Float(c) if (*c - 0.0125).abs() < f64::EPSILON),
        "the priced spend is carried verbatim"
    );
}

/// The common path · a success with no OBS-E diagnostic emits NO
/// `warning` field (zero false-alarm noise on the happy stream).
#[test]
fn no_warning_field_on_a_clean_success() {
    let ran = task::RanTask {
        note: "exec · true".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::String("ok".to_owned()),
            tokens: None,
            recovered_from: None,
            warning: None,
            cost_usd: None,
            cost_unpriced: None,
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle_ran("t", ran, None, &mut ok, &mut stamper, &mut sink);

    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    assert!(
        !completed.fields.iter().any(|f| f.key == "warning"),
        "no warning on a clean success"
    );
    assert!(
        !completed.fields.iter().any(|f| f.key == "cost_usd"),
        "an unpriced success carries NO cost field — absent is honest, never a fake zero"
    );
    assert!(
        !completed.fields.iter().any(|f| f.key == "cost_unpriced"),
        "an exec success is not COST-unpriced — no reason noise on verbs that spend nothing"
    );
}

/// The WHY channel: an unpriced INFER success carries the reason —
/// `unknown` is never masked (a local model says « local compute ·
/// not priced », never a blank).
#[test]
fn cost_unpriced_reason_rides_task_completed() {
    let ran = task::RanTask {
        note: "infer · ollama/llama3.2".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::String("bonjour".to_owned()),
            tokens: Some(12),
            recovered_from: None,
            warning: None,
            cost_usd: None,
            cost_unpriced: Some(nika_types::cost::UnpricedReason::LocalModel),
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle_ran("ask", ran, None, &mut ok, &mut stamper, &mut sink);

    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    assert!(
        !completed.fields.iter().any(|f| f.key == "cost_usd"),
        "no fake zero next to the reason"
    );
    let reason = completed
        .fields
        .iter()
        .find(|f| f.key == "cost_unpriced")
        .expect("the WHY rides the frame");
    assert!(
        matches!(&reason.value, FieldValue::String(s) if s == "local_model"),
        "snake_case wire form"
    );
}

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

mod recover_await {
    use std::num::NonZeroUsize;

    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_invoke::InvokeVerb;

    use super::*;

    /// The real parse → check → run chain over mock seams (the `spec_v2`
    /// harness idiom, exec-only — every fixture here drives the shell).
    async fn run_yaml(
        yaml: &str,
        shell: MockShell,
        cap: Option<usize>,
    ) -> (RunOutcome, Vec<Event>) {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
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
nika: v1
workflow:
  id: recover-await-same-wave
tasks:
  risky:
    exec: { shell: "exit 1" }
    on_error:
      recover: ${{ tasks.source.output }}
    output:
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
nika: v1
workflow:
  id: recover-await-chain
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
    after: { base: succeeded }
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
nika: v1
workflow:
  id: recover-await-skipped
tasks:
  risky:
    exec: { shell: "exit 1" }
    on_error:
      recover: ${{ tasks.source.output }}
  base:
    exec: { command: ["echo", "base"] }
  source:
    after: { base: succeeded }
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
nika: v1
workflow:
  id: recover-await-mutual
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
nika: v1
workflow:
  id: recover-terminal-broken-path
tasks:
  done:
    exec: { command: ["echo", "ok"] }
  risky:
    after: { done: succeeded }
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
        let yaml =
            "nika: v1\nworkflow:\n  id: t\ntasks:\n  risky:\n    exec: { shell: \"exit 1\" }\n";
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let (vars, env, secrets) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        let resume_ctx = resume::ResumeContext::of(&wf, &secrets, None, &BTreeMap::new());
        let scope = crate::recover::ResolveScope {
            wf: &wf,
            vars: &vars,
            env: &env,
            secrets: &secrets,
            resume_ctx: &resume_ctx,
        };
        let error = |code: &str, message: &str| TaskErrorRecord {
            code: code.to_owned(),
            message: message.to_owned(),
            transient: false,
        };
        let pending = crate::recover::PendingRecovery {
            failed: task::FailedOutcome {
                record: error("NIKA-EXEC-001", "exit 1"),
                cost_usd: None,
                cost_unpriced: None,
            },
            render_error: error("NIKA-VAR-001", "unresolved reference `tasks.ghost.output`"),
            awaiting: std::collections::BTreeSet::from(["ghost".to_owned()]),
            with_ns: BTreeMap::new(),
        };
        let finish = task::Finish {
            id: "risky".to_owned(),
            settle: task::SettleAs::Ran(task::RanTask {
                note: "exec · sh".to_owned(),
                retries: Vec::new(),
                agent_events: Vec::new(),
                duration_ms: 0,
                result: task::RunResult::PendingRecovery(Box::new(pending)),
            }),
            named: BTreeMap::new(),
            resume: None,
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
        assert_eq!(records["risky"].status, TaskStatus::Failure);
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
nika: v1
workflow:
  id: recover-fanout-boundary
tasks:
  fan:
    for_each: ["x"]
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
}

/// #473 — the `skills:` composition seam: the composer-resolved SKILL.md
/// texts join the agent's SYSTEM context as the normative `## Skills`
/// section (spec 02 §agent skills), asserted at the provider seam (the
/// message bytes the model actually receives); an unresolved reference
/// fails the TASK with the check-time code (check≡run).
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod skill_compose_tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use nika_kernel::provider::Role;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_invoke::InvokeVerb;

    use crate::dispatch::system_with_skills;
    use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

    #[test]
    fn skills_section_shape_is_deterministic() {
        let docs = vec![
            nika_schema::SkillDoc::new("alpha", "First skill.", "\n# Alpha\n\nDo alpha things.\n"),
            nika_schema::SkillDoc::new("beta", "Second skill.", ""),
        ];
        // With an authored system — the section appends after ONE blank line.
        let with_system = system_with_skills(Some("You are helpful.".to_owned()), &docs);
        assert_eq!(
            with_system,
            "You are helpful.\n\n## Skills\n\n### alpha\n\nFirst skill.\n\n# Alpha\n\nDo alpha things.\n\n### beta\n\nSecond skill.",
            "the injection bytes are the documented shape"
        );
        // Without one — the section IS the system prompt.
        let bare = system_with_skills(None, &docs[..1]);
        assert!(bare.starts_with("## Skills\n\n### alpha"), "{bare}");
    }

    const SKILL_MD: &str =
        "---\nname: reviewer\ndescription: Review with care.\n---\n\nAlways review twice.\n";

    fn wf_with_skill() -> nika_schema::raw::RawWorkflow {
        nika_schema::parse(
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  go:\n    agent:\n      system: \"Base system.\"\n      prompt: \"hello\"\n      skills: [\"skills/reviewer/SKILL.md\"]\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn runtime_with(
        provider: MockProvider,
        skills: BTreeMap<String, String>,
    ) -> Runtime<
        MockShell,
        MockToolExecutor,
        nika_providers::NoHttp,
        MockProvider,
        MockToolDefinitionProvider,
        MockClock,
    > {
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            nika_verb_infer::InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "mock/echo",
            ),
            AgentVerb::new(
                Arc::new(provider),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        )
        .with_skills(skills)
    }

    #[tokio::test]
    async fn resolved_skills_reach_the_provider_system_message() {
        let wf = wf_with_skill();
        let report = nika_schema::check(&wf);
        assert!(report.is_clean(), "the static ladder is fs-free — clean");
        let provider = MockProvider::new("mock").enqueue_text("done");
        let probe = provider.clone();
        let runtime = runtime_with(
            provider,
            BTreeMap::from([("skills/reviewer/SKILL.md".to_owned(), SKILL_MD.to_owned())]),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("run settles");
        assert!(outcome.ok, "the mock loop settles green");

        let requests = probe.captured_requests();
        assert_eq!(requests.len(), 1, "one provider turn");
        let system = &requests[0].messages[0];
        assert!(matches!(system.role, Role::System), "system leads");
        let text = match &system.content[0] {
            nika_kernel::provider::ContentBlock::Text { text } => text.clone(),
            other => panic!("system is text: {other:?}"),
        };
        assert!(
            text.starts_with("Base system.\n\n## Skills\n\n### reviewer\n\nReview with care."),
            "the authored system + the normative section: {text}"
        );
        assert!(
            text.contains("Always review twice."),
            "the skill BODY rides along: {text}"
        );
    }

    #[tokio::test]
    async fn unresolved_skill_fails_the_task_with_the_check_code() {
        // An embedder that skipped `with_skills` — the task fails loudly
        // with the same code `nika check` teaches (NIKA-AGENT-003), and
        // NO provider call is ever made (fail BEFORE spend).
        let wf = wf_with_skill();
        let report = nika_schema::check(&wf);
        let provider = MockProvider::new("mock").enqueue_text("never reached");
        let probe = provider.clone();
        let runtime = runtime_with(provider, BTreeMap::new());
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("run settles");
        assert!(!outcome.ok, "the task fails");
        let record = outcome.records.get("go").expect("record exists");
        let error = record.error.as_ref().expect("failure carries the error");
        assert_eq!(error.code, "NIKA-AGENT-003");
        assert!(error.message.contains("skills/reviewer/SKILL.md"));
        assert!(!error.transient, "a composition defect never retries");
        assert!(
            probe.captured_requests().is_empty(),
            "no token is spent on a broken composition"
        );
    }

    #[tokio::test]
    async fn invalid_skill_text_fails_the_task_with_the_defect_code() {
        // A text that reaches dispatch but is NOT a valid Agent Skill —
        // the NIKA-AGENT-004 voice (same defect wording as nika check).
        let wf = wf_with_skill();
        let report = nika_schema::check(&wf);
        let provider = MockProvider::new("mock").enqueue_text("never reached");
        let runtime = runtime_with(
            provider,
            BTreeMap::from([(
                "skills/reviewer/SKILL.md".to_owned(),
                "# no frontmatter here\n".to_owned(),
            )]),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("run settles");
        assert!(!outcome.ok);
        let error = outcome.records["go"].error.as_ref().expect("the error");
        assert_eq!(error.code, "NIKA-AGENT-004");
        assert!(error.message.contains("frontmatter"), "{}", error.message);
    }
}
