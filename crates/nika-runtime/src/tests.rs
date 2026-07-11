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

    let yaml = "nika: v1\nworkflow: stream-settle\ntasks:\n  - id: fast\n    exec: { command: \"true\" }\n  - id: gate\n    invoke: { tool: \"nika:jq\", args: { input: [], expression: \".gate\" } }\n";
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
workflow: vals
env:
  API_BASE: "https://api.example.test"
vars:
  plain: "text"
  urls: ["a", "b"]
  topic: { type: string, default: "news" }
tasks:
  - id: t
    exec: { command: "true" }
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
workflow: typed-out
tasks:
  - id: t
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
            recovered_from: Some("NIKA-EXEC-001".to_owned()),
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
